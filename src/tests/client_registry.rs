use super::*;
use std::future::{Future, pending};
use std::task::{Context, Poll, Waker};

fn cached_client() -> Result<(Arc<ClientCache>, ClientKey), String> {
    let key = (
        Channel::Stable,
        "px_9a".to_string(),
        vec!["arm64-v8a".to_string()],
    );
    let client = GooglePlayClient::new_for_abis(
        "px_9a",
        &key.2,
        "test@example.test",
        "test-token",
        Channel::Stable,
    )?;
    let cache = Arc::new(Mutex::new(HashMap::from([(
        key.clone(),
        ClientEntry {
            client,
            logged_in_at_ms: Some(100),
        },
    )])));
    Ok((cache, key))
}

#[test]
fn abandoned_request_does_not_retain_cache_lock() -> Result<(), String> {
    let (cache, key) = cached_client()?;
    let request_cache = Arc::clone(&cache);
    let request_key = key.clone();
    let mut request = Box::pin(async move {
        let entry = snapshot(&request_cache, &request_key)?;
        pending::<()>().await;
        Ok::<_, String>(entry)
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
fn request_session_mutations_do_not_modify_the_cache() -> Result<(), String> {
    let (cache, key) = cached_client()?;
    let mut request = snapshot(&cache, &key)?.ok_or("Missing test entry")?;
    request.logged_in_at_ms = Some(500);
    assert_eq!(
        snapshot(&cache, &key)?.and_then(|entry| entry.logged_in_at_ms),
        Some(100)
    );
    // The copy is published only after a completed refresh.
    cache
        .lock()
        .map_err(|_| "Poisoned test cache")?
        .insert(key.clone(), request);
    assert_eq!(
        snapshot(&cache, &key)?.and_then(|entry| entry.logged_in_at_ms),
        Some(500)
    );
    Ok(())
}
