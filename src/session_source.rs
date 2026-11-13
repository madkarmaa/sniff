//! Finds a logged-in Google Play client. It tries this isolate's cache
//! (`session_cache`), then the session shared in KV (`session_store`), then a
//! renewal on the stored device, and checks a new device in only as a last
//! resort.

use crate::api_error::{ApiError, Stage};
use crate::google_play_client::{Channel, GooglePlayClient, UpstreamError};
use crate::session_cache::{ClientCache, ClientEntry, ClientKey, GLOBAL_CLIENTS, Session};
use crate::session_cache::{evict, lock, snapshot};
use crate::session_store::{self, StoredSession, is_usable, storage_key};
use gpapi::SessionState;
use worker::kv::KvStore;
use worker::{Date, Env, console_error, console_log};

/// Milliseconds since the epoch. (`std::time::Instant` cannot be used here:
/// the Workers runtime does not implement it and it panics.)
fn now_ms() -> u64 {
    Date::now().as_millis()
}

/// Log an upstream failure and classify it for the client.
pub fn upstream_error(stage: Stage, channel: Channel, error: &UpstreamError) -> ApiError {
    // Detail (including response excerpts) is for the logs only.
    console_error!("{stage:?} failed on the {channel} channel: {error}");
    ApiError::from_upstream(stage, channel, &**error)
}

/// The account a channel logs in with.
pub struct Credentials {
    pub email: String,
    pub aas_token: String,
}

impl Credentials {
    fn client(&self, (_, device_name, abis): &ClientKey) -> Result<GooglePlayClient, ApiError> {
        GooglePlayClient::new_for_abis(device_name, abis, &self.email, &self.aas_token)
    }
}

pub struct SessionSource {
    clients: &'static ClientCache,
    /// Sessions shared across isolates; `None` when KV is not bound.
    store: Option<KvStore>,
}

impl SessionSource {
    #[must_use]
    pub fn new(env: &Env) -> Self {
        Self {
            clients: &GLOBAL_CLIENTS,
            store: session_store::open(env),
        }
    }

    /// Get a logged-in client for `key`. `rejected_ms` is the login time of a
    /// session Google just rejected; only a newer one is reused.
    pub async fn get(
        &self,
        key: &ClientKey,
        credentials: &Credentials,
        rejected_ms: Option<u64>,
    ) -> Result<Session, ApiError> {
        // Only clone plain client/session state under the lock. A request must never
        // own an isolate-global lock while awaiting network or another request.
        let cached = snapshot(self.clients, key)?;
        if let Some(entry) = &cached
            && let Some(logged_in_at_ms) = entry.logged_in_at_ms
            && is_usable(logged_in_at_ms, now_ms(), rejected_ms)
        {
            return Ok(Session {
                client: entry.client.clone(),
                logged_in_at_ms,
            });
        }

        let mut client = credentials.client(key)?;
        if let Some((stored, state)) = self.load(key, &credentials.email).await {
            client.restore_session(state);
            if is_usable(stored.logged_in_at_ms, now_ms(), rejected_ms) {
                return self.remember(key, client, stored.logged_in_at_ms);
            }
        } else if let Some(entry) = cached {
            // KV lost the record (or is unbound) but this isolate knows the device.
            client = entry.client;
        }
        self.renew(key, credentials, client).await
    }

    /// Drop a session Google rejected. When it was a renewal that got rejected
    /// too, `forget_device` also drops the stored device, so the next request
    /// checks in a fresh one instead of renewing a refused device forever.
    pub async fn discard(
        &self,
        key: &ClientKey,
        logged_in_at_ms: u64,
        forget_device: bool,
    ) -> Result<(), ApiError> {
        evict(self.clients, key, logged_in_at_ms)?;
        if forget_device && let Some(store) = &self.store {
            session_store::forget(store, key).await;
        }
        Ok(())
    }

    async fn load(&self, key: &ClientKey, email: &str) -> Option<(StoredSession, SessionState)> {
        let stored = session_store::load(self.store.as_ref()?, key, key.0).await?;
        let state = stored.state_for(email)?;
        Some((stored, state))
    }

    /// Renew `client`'s session on its registered device, or log in when it
    /// has none, then share the result with this isolate and KV.
    ///
    /// When Google refuses the registered device itself, this checks a fresh
    /// device in once, because the stored one would fail every later renewal.
    async fn renew(
        &self,
        key: &ClientKey,
        credentials: &Credentials,
        mut client: GooglePlayClient,
    ) -> Result<Session, ApiError> {
        let channel = key.0;
        let had_device = client.has_registered_device();
        if !had_device {
            console_log!(
                "Checking a new {channel} device in for {}",
                storage_key(key)
            );
        }
        let renewed = match client.reauthenticate().await {
            Err(error) if had_device && gpapi::error::is_auth_failure(&*error) => {
                console_log!("Stored {channel} device was refused ({error}); checking in anew");
                client = credentials.client(key)?;
                client.initialize().await
            }
            other => other,
        };
        renewed.map_err(|e| upstream_error(Stage::Login, channel, &e))?;

        let logged_in_at_ms = now_ms();
        if let (Some(store), Some(state)) = (&self.store, client.session_state()) {
            let stored = StoredSession::new(&credentials.email, state, logged_in_at_ms);
            session_store::save(store, key, &stored).await;
        }
        self.remember(key, client, logged_in_at_ms)
    }

    /// Cache a logged-in client in this isolate.
    fn remember(
        &self,
        key: &ClientKey,
        client: GooglePlayClient,
        logged_in_at_ms: u64,
    ) -> Result<Session, ApiError> {
        let entry = ClientEntry {
            client: client.clone(),
            logged_in_at_ms: Some(logged_in_at_ms),
        };
        lock(self.clients)?.insert(key.clone(), entry);
        Ok(Session {
            client,
            logged_in_at_ms,
        })
    }
}
