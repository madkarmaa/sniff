use super::*;
use std::future::{Future, pending};
use std::task::{Context, Poll, Waker};

fn cached_client() -> Result<(Arc<ClientCache>, ClientKey), ApiError> {
    let key = (
        Channel::Stable,
        "px_9a".to_string(),
        vec!["arm64-v8a".to_string()],
    );
    let client =
        GooglePlayClient::new_for_abis("px_9a", &key.2, "test@example.test", "test-token")?;
    let cache = Arc::new(Mutex::new(HashMap::from([(
        key.clone(),
        ClientEntry {
            client,
            logged_in_at_ms: Some(100),
        },
    )])));
    Ok((cache, key))
}

fn missing_entry() -> ApiError {
    ApiError::new(ErrorKind::Internal, "Missing test entry")
}

#[test]
fn abandoned_request_does_not_retain_cache_lock() -> Result<(), ApiError> {
    let (cache, key) = cached_client()?;
    let request_cache = Arc::clone(&cache);
    let request_key = key.clone();
    let mut request = Box::pin(async move {
        let entry = snapshot(&request_cache, &request_key)?;
        pending::<()>().await;
        Ok::<_, ApiError>(entry)
    });
    assert!(matches!(
        request
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
    // Model an owner future whose completion path is lost, not normal cancellation.
    std::mem::forget(request);
    assert!(cache.try_lock().is_ok());
    assert!(snapshot(&cache, &key)?.is_some());
    Ok(())
}

fn named(name: &str, url: &str) -> NamedDownload {
    NamedDownload {
        name: Some(name.into()),
        download_url: Some(url.into()),
        ..Default::default()
    }
}

#[test]
fn merge_takes_main_apk_metadata_from_the_info_that_supplied_the_url() {
    let without_main = DownloadInfo {
        main_apk_size: Some(1),
        main_apk_sha1: Some("stray".into()),
        ..Default::default()
    };
    let with_main = DownloadInfo {
        main_apk_url: Some("armv7/base".into()),
        main_apk_size: Some(32),
        main_apk_sha1: Some("armv7-sha1".into()),
        ..Default::default()
    };
    let merged = merge_download_infos(vec![without_main, with_main]);
    assert_eq!(merged.main_apk_url.as_deref(), Some("armv7/base"));
    assert_eq!(merged.main_apk_size, Some(32));
    assert_eq!(merged.main_apk_sha1.as_deref(), Some("armv7-sha1"));
    assert_eq!(merged.main_apk_sha256, None);
}

#[test]
fn merge_keeps_first_urls_and_deduplicates_by_name() {
    let first = DownloadInfo {
        main_apk_url: Some("arm64/base".into()),
        main_apk_size: Some(64),
        main_apk_sha1: Some("arm64-sha1".into()),
        main_apk_sha256: Some("arm64-sha256".into()),
        splits: vec![
            named("config.arm64_v8a", "arm64/abi"),
            named("config.en", "arm64/en"),
        ],
        additional_files: vec![named("main.1.app.obb", "arm64/obb")],
        dex_metadata_url: None,
    };
    let second = DownloadInfo {
        main_apk_url: Some("armv7/base".into()),
        main_apk_size: Some(32),
        main_apk_sha1: Some("armv7-sha1".into()),
        main_apk_sha256: Some("armv7-sha256".into()),
        splits: vec![
            named("config.armeabi_v7a", "armv7/abi"),
            named("config.en", "armv7/en"),
        ],
        additional_files: vec![named("main.1.app.obb", "armv7/obb")],
        dex_metadata_url: Some("armv7/dm".into()),
    };
    let merged = merge_download_infos(vec![first, second]);
    assert_eq!(merged.main_apk_url.as_deref(), Some("arm64/base"));
    assert_eq!(merged.main_apk_size, Some(64));
    assert_eq!(merged.main_apk_sha1.as_deref(), Some("arm64-sha1"));
    assert_eq!(merged.main_apk_sha256.as_deref(), Some("arm64-sha256"));
    assert_eq!(merged.dex_metadata_url.as_deref(), Some("armv7/dm"));
    let split_urls: Vec<_> = merged
        .splits
        .iter()
        .filter_map(|split| split.download_url.as_deref())
        .collect();
    assert_eq!(split_urls, ["arm64/abi", "arm64/en", "armv7/abi"]);
    assert_eq!(merged.additional_files.len(), 1);
}

#[test]
fn request_session_mutations_do_not_modify_the_cache() -> Result<(), ApiError> {
    let (cache, key) = cached_client()?;
    let mut request = snapshot(&cache, &key)?.ok_or_else(missing_entry)?;
    request.logged_in_at_ms = Some(500);
    assert_eq!(
        snapshot(&cache, &key)?.and_then(|entry| entry.logged_in_at_ms),
        Some(100)
    );
    // The copy is published only after a completed refresh.
    lock(&cache)?.insert(key.clone(), request);
    assert_eq!(
        snapshot(&cache, &key)?.and_then(|entry| entry.logged_in_at_ms),
        Some(500)
    );
    Ok(())
}

#[test]
fn evict_drops_the_rejected_session() -> Result<(), ApiError> {
    let (cache, key) = cached_client()?;
    evict(&cache, &key, 100)?;
    assert!(snapshot(&cache, &key)?.is_none());
    // The lock is released afterwards.
    assert!(cache.try_lock().is_ok());
    // Evicting an absent entry is not an error.
    evict(&cache, &key, 100)?;
    Ok(())
}

#[test]
fn evict_keeps_a_session_that_a_concurrent_request_already_replaced() -> Result<(), ApiError> {
    let (cache, key) = cached_client()?;
    let mut fresh = snapshot(&cache, &key)?.ok_or_else(missing_entry)?;
    fresh.logged_in_at_ms = Some(900);
    lock(&cache)?.insert(key.clone(), fresh);
    // This request saw the session from time 100 rejected, but it is gone.
    evict(&cache, &key, 100)?;
    assert_eq!(
        snapshot(&cache, &key)?.and_then(|entry| entry.logged_in_at_ms),
        Some(900)
    );
    Ok(())
}

#[test]
fn evict_leaves_other_keys_alone() -> Result<(), ApiError> {
    let (cache, key) = cached_client()?;
    let other = (Channel::Stable, "px_9a".to_string(), Vec::new());
    let entry = snapshot(&cache, &key)?.ok_or_else(missing_entry)?;
    lock(&cache)?.insert(other.clone(), entry);
    evict(&cache, &key, 100)?;
    assert!(snapshot(&cache, &key)?.is_none());
    assert!(snapshot(&cache, &other)?.is_some());
    Ok(())
}

fn info(main: &str) -> DownloadInfo {
    DownloadInfo {
        main_apk_url: Some(main.into()),
        splits: vec![named(&format!("config.{main}"), main)],
        ..Default::default()
    }
}

fn failure(kind: ErrorKind, message: &str) -> ApiError {
    ApiError::new(kind, message)
}

#[test]
fn download_targets_cover_every_abi_once() {
    let abis: Vec<_> = DOWNLOAD_TARGETS.iter().map(|target| target.abi).collect();
    assert_eq!(abis, ["arm64-v8a", "armeabi-v7a", "x86", "x86_64"]);
}

#[test]
fn collect_targets_reports_every_target_when_all_succeed() -> Result<(), ApiError> {
    let results = vec![Ok(info("a")), Ok(info("b")), Ok(info("c")), Ok(info("d"))];
    let (merged, outcomes) = collect_targets(&DOWNLOAD_TARGETS, results)?;
    assert_eq!(merged.splits.len(), 4);
    assert_eq!(outcomes.len(), DOWNLOAD_TARGETS.len());
    for (outcome, target) in outcomes.iter().zip(DOWNLOAD_TARGETS) {
        assert_eq!(outcome.target, target);
        assert!(outcome.succeeded());
        assert_eq!(outcome.error, None);
    }
    Ok(())
}

#[test]
fn collect_targets_flags_partial_merges() -> Result<(), ApiError> {
    let results = vec![
        Ok(info("a")),
        Err(failure(
            ErrorKind::Upstream,
            "Google Play returned HTTP 500",
        )),
        Ok(info("c")),
        Err(failure(ErrorKind::NotDownloadable, "cannot be downloaded")),
    ];
    let (merged, outcomes) = collect_targets(&DOWNLOAD_TARGETS, results)?;
    // Only the successful targets are merged...
    let split_names: Vec<_> = merged
        .splits
        .iter()
        .filter_map(|split| split.name.as_deref())
        .collect();
    assert_eq!(split_names, ["config.a", "config.c"]);
    // ...and the failed ones stay visible, in target order.
    let summary: Vec<_> = outcomes
        .iter()
        .map(|outcome| {
            (
                outcome.target.abi,
                outcome.succeeded(),
                outcome.error.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("arm64-v8a", true, None),
            ("armeabi-v7a", false, Some("Google Play returned HTTP 500")),
            ("x86", true, None),
            ("x86_64", false, Some("cannot be downloaded")),
        ]
    );
    Ok(())
}

#[test]
fn collect_targets_fails_only_when_every_target_failed() {
    let results = vec![
        Err(failure(ErrorKind::NotDownloadable, "no")),
        Err(failure(ErrorKind::NotDownloadable, "no")),
        Err(failure(ErrorKind::NotDownloadable, "no")),
        Err(failure(ErrorKind::NotDownloadable, "no")),
    ];
    assert_eq!(
        collect_targets(&DOWNLOAD_TARGETS, results)
            .err()
            .map(|e| e.status()),
        Some(422)
    );

    let results = vec![
        Err(failure(ErrorKind::NotDownloadable, "no")),
        Err(failure(ErrorKind::RateLimited, "slow down")),
        Err(failure(ErrorKind::Upstream, "bad gateway")),
        Err(failure(ErrorKind::NotDownloadable, "no")),
    ];
    assert_eq!(
        collect_targets(&DOWNLOAD_TARGETS, results)
            .err()
            .map(|e| e.status()),
        Some(503)
    );
}
