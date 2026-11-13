use crate::api_error::{ApiError, ErrorKind, Stage};
use crate::google_play_client::{Channel, GooglePlayClient, UpstreamError};
use futures_util::future::join_all;
use googleplay_protobuf::DetailsResponse;
use gpapi::{DownloadInfo, NamedDownload};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use worker::{Date, Env, console_error, console_log};

type ClientKey = (Channel, String, Vec<String>);

/// A device profile and the ABI it is made to advertise to Google Play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub device: &'static str,
    pub abi: &'static str,
}

const DOWNLOAD_TARGETS: [Target; 4] = [
    Target {
        device: "px_9a",
        abi: "arm64-v8a",
    },
    Target {
        device: "sm_a13_5g",
        abi: "armeabi-v7a",
    },
    Target {
        device: "google_kiwi_x86_64",
        abi: "x86",
    },
    Target {
        device: "google_kiwi_x86_64",
        abi: "x86_64",
    },
];

/// How long a logged-in client is reused before a fresh login is forced,
/// in milliseconds. Google-side session state expires; without this a stale
/// cache would fail every request until the isolate is recycled.
/// (`std::time::Instant` cannot be used here: the Workers runtime does not
/// implement it and it panics.)
const SESSION_TTL_MS: u64 = 1_800_000;

#[derive(Clone)]
struct ClientEntry {
    client: GooglePlayClient,
    logged_in_at_ms: Option<u64>,
}

/// A logged-in client plus the login time that identifies its cache entry,
/// so a rejected session can be evicted without evicting a newer one.
struct Session {
    client: GooglePlayClient,
    logged_in_at_ms: u64,
}

type ClientCache = Mutex<HashMap<ClientKey, ClientEntry>>;

fn lock(cache: &ClientCache) -> Result<MutexGuard<'_, HashMap<ClientKey, ClientEntry>>, ApiError> {
    cache.lock().map_err(|_| {
        console_error!("Client session cache lock is poisoned");
        ApiError::internal()
    })
}

fn snapshot(cache: &ClientCache, key: &ClientKey) -> Result<Option<ClientEntry>, ApiError> {
    lock(cache).map(|clients| clients.get(key).cloned())
}

/// Drop `key`'s entry if it is still the session logged in at `session_ms`.
/// A concurrent request may already have replaced a rejected session with a
/// fresh one; that one must survive. The lock is released before returning.
fn evict(cache: &ClientCache, key: &ClientKey, session_ms: u64) -> Result<(), ApiError> {
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

fn upstream_error(stage: Stage, channel: Channel, error: &UpstreamError) -> ApiError {
    // Detail (including response excerpts) is for the logs only.
    console_error!("{stage:?} failed on the {channel} channel: {error}");
    ApiError::from_upstream(stage, channel, &**error)
}

/// Everything a successful lookup reports.
pub struct Lookup {
    pub details: DetailsResponse,
    pub download: DownloadInfo,
    /// One entry per download target, in `DOWNLOAD_TARGETS` order.
    pub targets: Vec<TargetOutcome>,
}

/// Whether a download target contributed to the merged result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetOutcome {
    pub target: Target,
    /// Short client-safe message when the target failed.
    pub error: Option<String>,
}

impl TargetOutcome {
    #[must_use]
    pub const fn succeeded(&self) -> bool {
        self.error.is_none()
    }
}

pub struct ClientRegistry {
    clients: &'static ClientCache,
    env: Env,
}

impl ClientRegistry {
    #[must_use]
    pub fn new(env: Env) -> Self {
        Self {
            clients: &GLOBAL_CLIENTS,
            env,
        }
    }

    fn env_var(&self, name: &str) -> Option<String> {
        self.env
            .var(name)
            .ok()
            .map(|value| value.to_string())
            .filter(|value| !value.is_empty())
    }

    fn channel_credentials(&self, channel: Channel) -> Result<(String, String), ApiError> {
        let prefix = channel.to_string().to_uppercase();
        let email = self.env_var(&format!("{prefix}_EMAIL"));
        let aas_token = self.env_var(&format!("{prefix}_AAS_TOKEN"));
        if let (Some(email), Some(aas_token)) = (email, aas_token) {
            Ok((email, aas_token))
        } else {
            // Names are for the logs; clients only learn the channel.
            console_error!("{prefix}_EMAIL or {prefix}_AAS_TOKEN is not set");
            Err(ApiError::credentials_missing(channel))
        }
    }

    /// Get a logged-in client for `key`, reusing a cached session while fresh.
    async fn session(&self, key: &ClientKey) -> Result<Session, ApiError> {
        let (channel, device_name, abis) = key;
        // Only clone plain client/session state under the lock. A request must never
        // own an isolate-global lock while awaiting network or another request.
        let cached = snapshot(self.clients, key)?;
        let mut entry = if let Some(entry) = cached {
            if let Some(logged_in_at_ms) = entry.logged_in_at_ms
                && Date::now().as_millis().saturating_sub(logged_in_at_ms) < SESSION_TTL_MS
            {
                return Ok(Session {
                    client: entry.client,
                    logged_in_at_ms,
                });
            }
            entry
        } else {
            let (email, aas_token) = self.channel_credentials(*channel)?;
            ClientEntry {
                client: GooglePlayClient::new_for_abis(device_name, abis, &email, &aas_token)?,
                logged_in_at_ms: None,
            }
        };
        // Login mutates a request-local copy. Abandoning this future cannot strand
        // future requests on a shared asynchronous mutex.
        entry
            .client
            .initialize()
            .await
            .map_err(|e| upstream_error(Stage::Login, *channel, &e))?;
        let logged_in_at_ms = Date::now().as_millis();
        entry.logged_in_at_ms = Some(logged_in_at_ms);
        lock(self.clients)?.insert(key.clone(), entry.clone());
        Ok(Session {
            client: entry.client,
            logged_in_at_ms,
        })
    }

    /// Run `op` with a logged-in client for the channel/device/ABI combination.
    ///
    /// If Google rejects the session (HTTP 401/403) before the TTL lapsed, the
    /// cache entry is dropped, a fresh client logs in and `op` runs exactly
    /// once more. Each caller retries at most once, so concurrent download
    /// targets (distinct keys) cannot multiply logins. The error stays typed
    /// until this decision is made; only then is it turned into an `ApiError`.
    async fn with_session<T, Op, Fut>(
        &self,
        channel: Channel,
        device_name: &str,
        supported_abis: &[&str],
        stage: Stage,
        op: Op,
    ) -> Result<T, ApiError>
    where
        Op: Fn(GooglePlayClient) -> Fut,
        Fut: Future<Output = Result<T, UpstreamError>>,
    {
        let abis = supported_abis.iter().copied().map(String::from).collect();
        let key = (channel, device_name.to_string(), abis);

        let session = self.session(&key).await?;
        let rejected_session_ms = session.logged_in_at_ms;
        let error = match Box::pin(op(session.client)).await {
            Ok(value) => return Ok(value),
            Err(error) if gpapi::error::is_auth_failure(&*error) => error,
            Err(error) => return Err(upstream_error(stage, channel, &error)),
        };

        console_log!("Session rejected on the {channel} channel ({error}); logging in again");
        evict(self.clients, &key, rejected_session_ms)?;
        // The entry is gone (or was already replaced by a concurrent request's
        // fresh login), so this either logs in anew or reuses that fresh login.
        let session = self.session(&key).await?;
        Box::pin(op(session.client))
            .await
            .map_err(|error| upstream_error(stage, channel, &error))
    }

    /// Get latest app details and merged download info across download targets.
    ///
    /// All download targets use the version advertised in the returned details.
    /// Beta and alpha lookups stop early when the app has no testing program
    /// the channel's account is enrolled in.
    ///
    /// # Errors
    ///
    /// Returns an error if details cannot be fetched, the app is unknown or
    /// unavailable on the channel, the latest version cannot be determined, or
    /// all per-ABI downloads fail.
    pub async fn get_download_info(
        &self,
        package_name: &str,
        channel: Channel,
    ) -> Result<Lookup, ApiError> {
        let Some(device_name) = self.env_var("DEVICE_NAME") else {
            console_error!("DEVICE_NAME is not set");
            return Err(ApiError::misconfigured());
        };
        let details = self
            .with_session(
                channel,
                &device_name,
                &[],
                Stage::Details,
                |client| async move { client.get_details(package_name).await },
            )
            .await?
            .ok_or_else(|| ApiError::not_found(package_name))?;
        if !channel.is_available_in(&details) {
            return Err(ApiError::channel_unavailable(channel, package_name));
        }
        let version_code = details
            .item
            .as_ref()
            .and_then(|item| item.details.as_ref())
            .and_then(|details| details.app_details.as_ref())
            .and_then(|app_details| app_details.version_code)
            .ok_or_else(|| {
                console_error!("No version code in the details of '{package_name}'");
                ApiError::new(
                    ErrorKind::Upstream,
                    format!("Google Play did not report a version for '{package_name}'"),
                )
            })?;

        // Targets are fetched concurrently; `join_all` keeps their order, which
        // decides which target's main APK and dex metadata win the merge.
        // Boxed: each download future is ~90kB, too large to hold inline.
        let results = join_all(DOWNLOAD_TARGETS.map(|target| {
            Box::pin(async move {
                self.with_session(
                    channel,
                    target.device,
                    &[target.abi],
                    Stage::Download,
                    |client| async move {
                        client
                            .get_download_info(package_name, Some(version_code))
                            .await
                    },
                )
                .await
            })
        }))
        .await;

        let (download, targets) = collect_targets(&DOWNLOAD_TARGETS, results)?;
        Ok(Lookup {
            details,
            download,
            targets,
        })
    }
}

/// Merge the targets that succeeded and report the outcome of every target.
/// Fails only when all of them failed, with the most telling error.
fn collect_targets(
    targets: &[Target],
    results: Vec<Result<DownloadInfo, ApiError>>,
) -> Result<(DownloadInfo, Vec<TargetOutcome>), ApiError> {
    let mut download_infos = Vec::new();
    let mut errors = Vec::new();
    let mut outcomes = Vec::new();
    for (target, result) in targets.iter().copied().zip(results) {
        match result {
            Ok(download_info) => {
                download_infos.push(download_info);
                outcomes.push(TargetOutcome {
                    target,
                    error: None,
                });
            }
            Err(error) => {
                outcomes.push(TargetOutcome {
                    target,
                    error: Some(error.message().to_string()),
                });
                errors.push(error);
            }
        }
    }

    if download_infos.is_empty() {
        Err(ApiError::aggregate(&errors))
    } else {
        Ok((merge_download_infos(download_infos), outcomes))
    }
}

#[must_use]
fn merge_download_infos(download_infos: Vec<DownloadInfo>) -> DownloadInfo {
    let mut merged = DownloadInfo::default();
    let mut split_names = HashSet::new();
    let mut additional_filenames = HashSet::new();

    for info in download_infos {
        // Size and hashes must come from the same info as the URL they describe.
        if merged.main_apk_url.is_none() && info.main_apk_url.is_some() {
            merged.main_apk_url = info.main_apk_url;
            merged.main_apk_size = info.main_apk_size;
            merged.main_apk_sha1 = info.main_apk_sha1;
            merged.main_apk_sha256 = info.main_apk_sha256;
        }
        merged.dex_metadata_url = merged.dex_metadata_url.or(info.dex_metadata_url);

        for split in info.splits {
            push_unique(&mut merged.splits, &mut split_names, split);
        }

        for file in info.additional_files {
            push_unique(
                &mut merged.additional_files,
                &mut additional_filenames,
                file,
            );
        }
    }

    merged
}

/// Push `item` unless one with the same name (or URL, when unnamed) was already pushed.
fn push_unique(target: &mut Vec<NamedDownload>, seen: &mut HashSet<String>, item: NamedDownload) {
    let key = item
        .name
        .as_ref()
        .or(item.download_url.as_ref())
        .cloned()
        .unwrap_or_default();
    if seen.insert(key) {
        target.push(item);
    }
}

pub type SharedClientRegistry = Arc<ClientRegistry>;

// Only completed, cloneable client configuration and session tokens are cached.
// In WASM reqwest::Client contains HTTP configuration, not request promises/streams.
// Each request binds its own Env and releases synchronous cache locks before await.
static GLOBAL_CLIENTS: LazyLock<ClientCache> = LazyLock::new(ClientCache::default);

#[must_use]
pub fn shared_registry(env: &Env) -> SharedClientRegistry {
    Arc::new(ClientRegistry::new(env.clone()))
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn)]
#[path = "tests/client_registry.rs"]
mod tests;
