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

#[test]
fn merge_keeps_first_urls_and_deduplicates_by_name() {
    let first = DownloadInfo {
        main_apk_url: Some("arm64/base".into()),
        splits: vec![
            (Some("config.arm64_v8a".into()), Some("arm64/abi".into())),
            (Some("config.en".into()), Some("arm64/en".into())),
        ],
        additional_files: vec![(Some("main.1.app.obb".into()), Some("arm64/obb".into()))],
        dex_metadata_url: None,
    };
    let second = DownloadInfo {
        main_apk_url: Some("armv7/base".into()),
        splits: vec![
            (Some("config.armeabi_v7a".into()), Some("armv7/abi".into())),
            (Some("config.en".into()), Some("armv7/en".into())),
        ],
        additional_files: vec![(Some("main.1.app.obb".into()), Some("armv7/obb".into()))],
        dex_metadata_url: Some("armv7/dm".into()),
    };
    let merged = merge_download_infos(vec![first, second]);
    assert_eq!(merged.main_apk_url.as_deref(), Some("arm64/base"));
    assert_eq!(merged.dex_metadata_url.as_deref(), Some("armv7/dm"));
    let split_urls: Vec<_> = merged
        .splits
        .iter()
        .filter_map(|(_, url)| url.as_deref())
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
