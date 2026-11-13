use super::*;
use crate::api_error::ErrorKind;
use std::future::{Future, pending};
use std::sync::Arc;
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
