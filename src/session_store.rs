//! Google Play sessions persisted in Workers KV, shared by every isolate.
//!
//! Each login checks a new device in on the channel's Google account. With
//! only the isolate-local cache (`session_cache`), every cold isolate did
//! that, so the accounts collected a new device per isolate. This module
//! stores the registered device and its current session for each channel,
//! device and ABI combination. An isolate resumes the stored session, renews
//! it on the same device once it is stale, and checks a device in only when
//! there is no usable one.
//!
//! When the binding is missing or a KV call fails, the worker logs it and
//! logs in afresh, as before.

use crate::google_play_client::Channel;
use crate::session_cache::ClientKey;
use gpapi::SessionState;
use serde::{Deserialize, Serialize};
use worker::{Env, console_error, console_warn, kv::KvStore};

/// The KV namespace binding. Wrangler provisions it on deploy.
pub const BINDING: &str = "SESSIONS";

/// How long a logged-in session is used before it is renewed on the same
/// device, in milliseconds. Google-side session state expires; without this a
/// stale session would fail every request until it is rejected.
pub const SESSION_TTL_MS: u64 = 1_800_000;

/// How long an unused device is remembered, in seconds. Every renewal writes
/// the record again, so only devices nobody asked for in this long expire.
const DEVICE_TTL_SECS: u64 = 30 * 24 * 60 * 60;

/// Bumped whenever the record format or its meaning changes.
const KEY_PREFIX: &str = "session:v1";

/// The KV key for `key`, made of the channel, device codename and advertised ABIs.
#[must_use]
pub fn storage_key((channel, device, abis): &ClientKey) -> String {
    format!("{KEY_PREFIX}:{channel}:{device}:{}", abis.join(","))
}

/// Whether a session logged in at `logged_in_at_ms` may be used at `now_ms`.
///
/// `rejected_ms` is the login time of a session Google just rejected. Only a
/// newer session, logged in by a concurrent request, is worth trying.
#[must_use]
pub const fn is_usable(logged_in_at_ms: u64, now_ms: u64, rejected_ms: Option<u64>) -> bool {
    let fresh = now_ms.saturating_sub(logged_in_at_ms) < SESSION_TTL_MS;
    let newer = match rejected_ms {
        Some(rejected_ms) => logged_in_at_ms > rejected_ms,
        None => true,
    };
    fresh && newer
}

/// A stored session, as JSON. The Android ID is a string because JSON numbers
/// lose precision beyond 2^53 once JavaScript reads them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredSession {
    /// The account the device is registered on; a record for another account
    /// (the channel's credentials changed) is ignored.
    email: String,
    gsf_id: String,
    device_checkin_consistency_token: Option<String>,
    device_config_token: String,
    auth_token: Option<String>,
    dfe_cookie: Option<String>,
    pub logged_in_at_ms: u64,
}

impl StoredSession {
    #[must_use]
    pub fn new(email: &str, state: SessionState, logged_in_at_ms: u64) -> Self {
        Self {
            email: email.to_string(),
            gsf_id: state.gsf_id.to_string(),
            device_checkin_consistency_token: state.device_checkin_consistency_token,
            device_config_token: state.device_config_token,
            auth_token: state.auth_token,
            dfe_cookie: state.dfe_cookie,
            logged_in_at_ms,
        }
    }

    /// The session to restore, if this record belongs to `email` and is intact.
    #[must_use]
    pub fn state_for(&self, email: &str) -> Option<SessionState> {
        if self.email != email {
            return None;
        }
        Some(SessionState {
            gsf_id: self.gsf_id.parse().ok()?,
            device_checkin_consistency_token: self.device_checkin_consistency_token.clone(),
            device_config_token: self.device_config_token.clone(),
            auth_token: self.auth_token.clone(),
            dfe_cookie: self.dfe_cookie.clone(),
        })
    }
}

/// The KV namespace, or `None` (logged) when this deployment has none.
#[must_use]
pub fn open(env: &Env) -> Option<KvStore> {
    match env.kv(BINDING) {
        Ok(store) => Some(store),
        Err(e) => {
            console_warn!("KV binding {BINDING} is unavailable, sessions stay per isolate: {e}");
            None
        }
    }
}

/// The stored session for `key`, if any could be read.
pub async fn load(store: &KvStore, key: &ClientKey, channel: Channel) -> Option<StoredSession> {
    let text = match store.get(&storage_key(key)).text().await {
        Ok(text) => text?,
        Err(e) => {
            console_error!("Reading the stored {channel} session failed: {e}");
            return None;
        }
    };
    serde_json::from_str(&text)
        .inspect_err(|e| console_error!("Stored {channel} session is unreadable: {e}"))
        .ok()
}

/// Store `session` for `key`, renewing the device's expiry.
pub async fn save(store: &KvStore, key: &ClientKey, session: &StoredSession) {
    let channel = key.0;
    let text = match serde_json::to_string(session) {
        Ok(text) => text,
        Err(e) => {
            console_error!("The {channel} session could not be encoded: {e}");
            return;
        }
    };
    let put = match store.put(&storage_key(key), text) {
        Ok(put) => put.expiration_ttl(DEVICE_TTL_SECS),
        Err(e) => {
            console_error!("Storing the {channel} session failed: {e}");
            return;
        }
    };
    if let Err(e) = put.execute().await {
        console_error!("Storing the {channel} session failed: {e}");
    }
}

/// Forget the device for `key`, so the next login registers a fresh one.
pub async fn forget(store: &KvStore, key: &ClientKey) {
    if let Err(e) = store.delete(&storage_key(key)).await {
        console_error!("Deleting the stored {} session failed: {e}", key.0);
    }
}

#[cfg(test)]
// Tests use assertions for contract failures and `?` for parse errors.
#[allow(clippy::panic_in_result_fn)]
#[path = "tests/session_store.rs"]
mod tests;
