use crate::api_error::ApiError;
use crate::google_play_client::{Channel, GooglePlayClient};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard};
use worker::console_error;

pub type ClientKey = (Channel, String, Vec<String>);

#[derive(Clone)]
pub struct ClientEntry {
    pub client: GooglePlayClient,
    pub logged_in_at_ms: Option<u64>,
}

/// A logged-in client plus the login time that identifies its cache entry,
/// so a rejected session can be evicted without evicting a newer one.
pub struct Session {
    pub client: GooglePlayClient,
    pub logged_in_at_ms: u64,
}

pub type ClientCache = Mutex<HashMap<ClientKey, ClientEntry>>;

pub fn lock(
    cache: &ClientCache,
) -> Result<MutexGuard<'_, HashMap<ClientKey, ClientEntry>>, ApiError> {
    cache.lock().map_err(|_| {
        console_error!("Client session cache lock is poisoned");
        ApiError::internal()
    })
}

pub fn snapshot(cache: &ClientCache, key: &ClientKey) -> Result<Option<ClientEntry>, ApiError> {
    lock(cache).map(|clients| clients.get(key).cloned())
}

/// Drop `key`'s entry if it is still the session logged in at `session_ms`.
/// A concurrent request may already have replaced a rejected session with a
/// fresh one; that one must survive. The lock is released before returning.
pub fn evict(cache: &ClientCache, key: &ClientKey, session_ms: u64) -> Result<(), ApiError> {
    let mut clients = lock(cache)?;
    if clients
        .get(key)
        .is_some_and(|entry| entry.logged_in_at_ms == Some(session_ms))
    {
        clients.remove(key);
    }
    drop(clients);
    Ok(())
}

// Only completed, cloneable client configuration and session tokens are cached.
// In WASM reqwest::Client contains HTTP configuration, not request promises/streams.
// Each request binds its own Env and releases synchronous cache locks before await.
pub static GLOBAL_CLIENTS: LazyLock<ClientCache> = LazyLock::new(ClientCache::default);

#[cfg(test)]
#[allow(clippy::panic_in_result_fn)]
#[path = "tests/session_cache.rs"]
mod tests;
