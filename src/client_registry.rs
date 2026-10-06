use gpapi::DownloadInfo;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use worker::{Date, Env, console_log};

use crate::google_play_client::{Channel, GooglePlayClient};

type ClientKey = (Channel, String, Vec<String>);

const DOWNLOAD_TARGETS: [(&str, &str); 4] = [
    ("px_9a", "arm64-v8a"),
    ("sm_a13_5g", "armeabi-v7a"),
    ("google_kiwi_x86_64", "x86"),
    ("google_kiwi_x86_64", "x86_64"),
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

type ClientCache = Mutex<HashMap<ClientKey, ClientEntry>>;

fn snapshot(cache: &ClientCache, key: &ClientKey) -> Result<Option<ClientEntry>, String> {
    cache
        .lock()
        .map_err(|_| "Client session cache is unavailable".to_string())
        .map(|clients| clients.get(key).cloned())
}

pub struct ClientRegistry {
    clients: Arc<ClientCache>,
    env: Env,
}

impl ClientRegistry {
    #[must_use]
    pub fn new(env: Env) -> Self {
        Self {
            clients: GLOBAL_CLIENTS
                .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
                .clone(),
            env,
        }
    }

    fn env_var(&self, name: &str) -> Result<String, String> {
        self.env
            .var(name)
            .map_err(|e| format!("missing {name} env: {e:?}"))
            .map(|v| v.to_string())
    }

    fn channel_credentials(&self, channel: Channel) -> Result<(String, String), String> {
        let prefix = channel.to_string().to_uppercase();
        Ok((
            self.env_var(&format!("{prefix}_EMAIL"))?,
            self.env_var(&format!("{prefix}_AAS_TOKEN"))?,
        ))
    }

    /// Get a client for `channel` using the default device.
    ///
    /// # Errors
    ///
    /// Returns an error if required environment configuration is missing or
    /// if the client cannot be created or initialized.
    pub async fn get_client(&self, channel: Channel) -> Result<GooglePlayClient, String> {
        let device_name = self.env_var("DEVICE_NAME")?;
        self.get_client_for_device(channel, &device_name, &[]).await
    }

    async fn get_client_for_device(
        &self,
        channel: Channel,
        device_name: &str,
        supported_abis: &[&str],
    ) -> Result<GooglePlayClient, String> {
        let abis: Vec<String> = supported_abis.iter().copied().map(String::from).collect();
        let key = (channel, device_name.to_string(), abis.clone());
        // Only clone plain client/session state under the lock. A request must never
        // own an isolate-global lock while awaiting network or another request.
        let cached = snapshot(&self.clients, &key)?;
        let mut entry = if let Some(entry) = cached {
            if entry.logged_in_at_ms.is_some_and(|logged_in_at_ms| {
                Date::now().as_millis().saturating_sub(logged_in_at_ms) < SESSION_TTL_MS
            }) {
                return Ok(entry.client);
            }
            entry
        } else {
            let (email, aas_token) = self.channel_credentials(channel)?;
            ClientEntry {
                client: GooglePlayClient::new_for_abis(
                    device_name,
                    &abis,
                    &email,
                    &aas_token,
                    channel,
                )?,
                logged_in_at_ms: None,
            }
        };
        // Login mutates a request-local copy. Abandoning this future cannot strand
        // future requests on a shared asynchronous mutex.
        entry.client.initialize().await?;
        entry.logged_in_at_ms = Some(Date::now().as_millis());
        self.clients
            .lock()
            .map_err(|_| "Client session cache is unavailable".to_string())?
            .insert(key, entry.clone());
        Ok(entry.client)
    }

    /// Get details for `package_name` on `channel`.
    ///
    /// # Errors
    ///
    /// Returns an error if the channel is unavailable for the package or if
    /// the underlying client request fails.
    pub async fn get_details_with_fallback(
        &self,
        package_name: &str,
        channel: Channel,
    ) -> Result<Option<(Channel, googleplay_protobuf::DetailsResponse)>, String> {
        if !channel.is_available_for_package(package_name) {
            return Err(format!(
                "Channel '{channel}' is not available for package '{package_name}'"
            ));
        }

        let client = self.get_client(channel).await?;
        client
            .get_details(package_name)
            .await
            .map(|opt| opt.map(|response| (channel, response)))
    }

    /// Get details across all available channels.
    ///
    /// Returns `Ok(None)` when the app is not found on the stable channel.
    ///
    /// # Errors
    ///
    /// Returns an error if the stable channel lookup fails.
    pub async fn get_details_multi(
        &self,
        package_name: &str,
    ) -> Result<Option<HashMap<Channel, googleplay_protobuf::DetailsResponse>>, String> {
        let mut results = HashMap::new();

        match self
            .get_details_with_fallback(package_name, Channel::Stable)
            .await
        {
            Ok(Some((_, response))) => {
                results.insert(Channel::Stable, response);
            }
            Ok(None) => return Ok(None),
            Err(e) => {
                console_log!("Error fetching {package_name} for stable channel: {e}");
                return Err(e);
            }
        }

        // Beta/Alpha credentials are optional: if they are missing (or the
        // channel errors), skip the channel instead of failing the request.
        for channel in [Channel::Beta, Channel::Alpha] {
            if channel.is_available_for_package(package_name) {
                self.try_insert_optional(&mut results, package_name, channel)
                    .await;
            }
        }

        Ok(Some(results))
    }

    async fn try_insert_optional(
        &self,
        results: &mut HashMap<Channel, googleplay_protobuf::DetailsResponse>,
        package_name: &str,
        channel: Channel,
    ) {
        match self.get_client(channel).await {
            Err(e) => {
                console_log!("Skipping {channel} channel for {package_name}: {e}");
            }
            Ok(client) => match client.get_details(package_name).await {
                Ok(Some(response)) => {
                    results.insert(channel, response);
                }
                Err(e) => {
                    console_log!("Error fetching {package_name} for {channel} channel: {e}");
                }
                Ok(None) => {}
            },
        }
    }

    /// Get merged download info across download targets.
    ///
    /// # Errors
    ///
    /// Returns an error if the channel is unavailable for the package or if
    /// all per-ABI download attempts fail.
    pub async fn get_download_info(
        &self,
        package_name: &str,
        channel: Channel,
        version_code: Option<i64>,
    ) -> Result<Option<(Channel, DownloadInfo)>, String> {
        if !channel.is_available_for_package(package_name) {
            return Err(format!(
                "Channel '{channel}' is not available for package '{package_name}'"
            ));
        }

        let mut download_infos = Vec::new();
        let mut errors = Vec::new();

        for (device_name, abi) in DOWNLOAD_TARGETS {
            let result = match self
                .get_client_for_device(channel, device_name, &[abi])
                .await
            {
                // Boxed: the download future is ~90kB, too large to hold inline.
                Ok(client) => Box::pin(client.get_download_info(package_name, version_code)).await,
                Err(error) => Err(error),
            };

            match result {
                Ok(download_info) => download_infos.push(download_info),
                Err(error) => {
                    console_log!("Error fetching {abi} download for {package_name}: {error}");
                    errors.push(format!("{abi}: {error}"));
                }
            }
        }

        if download_infos.is_empty() {
            Err(errors.join("; "))
        } else {
            Ok(Some((channel, merge_download_infos(download_infos))))
        }
    }
}

#[must_use]
fn merge_download_infos(download_infos: Vec<DownloadInfo>) -> DownloadInfo {
    let mut main_apk_url = None;
    let mut splits = Vec::new();
    let mut additional_files = Vec::new();
    let mut dex_metadata_url = None;
    let mut split_names = HashSet::new();
    let mut additional_filenames = HashSet::new();

    for (main, device_splits, device_additional_files, dex_metadata) in download_infos {
        if main_apk_url.is_none() {
            main_apk_url = main;
        }
        if dex_metadata_url.is_none() {
            dex_metadata_url = dex_metadata;
        }

        for split in device_splits {
            push_unique(&mut splits, &mut split_names, split);
        }

        for file in device_additional_files {
            push_unique(&mut additional_files, &mut additional_filenames, file);
        }
    }

    (main_apk_url, splits, additional_files, dex_metadata_url)
}

fn push_unique(
    target: &mut Vec<(Option<String>, Option<String>)>,
    seen: &mut HashSet<String>,
    item: (Option<String>, Option<String>),
) {
    let key = item
        .0
        .clone()
        .or_else(|| item.1.clone())
        .unwrap_or_default();
    if seen.insert(key) {
        target.push(item);
    }
}

pub type SharedClientRegistry = Arc<ClientRegistry>;

// Only completed, cloneable client configuration and session tokens are cached.
// In WASM reqwest::Client contains HTTP configuration, not request promises/streams.
// Each request binds its own Env and releases synchronous cache locks before await.
static GLOBAL_CLIENTS: OnceLock<Arc<ClientCache>> = OnceLock::new();

#[must_use]
pub fn shared_registry(env: &Env) -> SharedClientRegistry {
    Arc::new(ClientRegistry::new(env.clone()))
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn)]
mod tests {
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
}
