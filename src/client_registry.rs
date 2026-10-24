use futures_util::future::join_all;
use googleplay_protobuf::DetailsResponse;
use gpapi::{DownloadInfo, NamedDownload};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
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

fn lock(cache: &ClientCache) -> Result<MutexGuard<'_, HashMap<ClientKey, ClientEntry>>, String> {
    cache
        .lock()
        .map_err(|_| "Client session cache is unavailable".to_string())
}

fn snapshot(cache: &ClientCache, key: &ClientKey) -> Result<Option<ClientEntry>, String> {
    lock(cache).map(|clients| clients.get(key).cloned())
}

/// Outcome of looking up an app's latest download on a channel.
pub enum DownloadLookup {
    Found(Box<(DetailsResponse, DownloadInfo)>),
    NotFound,
    /// The channel's account is not enrolled in the app's testing program.
    ChannelUnavailable,
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
        let cached = snapshot(self.clients, &key)?;
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
        lock(self.clients)?.insert(key, entry.clone());
        Ok(entry.client)
    }

    /// Get latest app details and merged download info across download targets.
    ///
    /// All download targets use the version advertised in the returned details.
    /// Beta and alpha lookups stop early when the app has no testing program
    /// the channel's account is enrolled in.
    ///
    /// # Errors
    ///
    /// Returns an error if details cannot be fetched, the latest version
    /// cannot be determined, or all per-ABI downloads fail.
    pub async fn get_download_info(
        &self,
        package_name: &str,
        channel: Channel,
    ) -> Result<DownloadLookup, String> {
        let Some(details) = self
            .get_client(channel)
            .await?
            .get_details(package_name)
            .await?
        else {
            return Ok(DownloadLookup::NotFound);
        };
        if !channel.is_available_in(&details) {
            return Ok(DownloadLookup::ChannelUnavailable);
        }
        let version_code = details
            .item
            .as_ref()
            .and_then(|item| item.details.as_ref())
            .and_then(|details| details.app_details.as_ref())
            .and_then(|app_details| app_details.version_code)
            .ok_or_else(|| format!("Latest version not found for app '{package_name}'"))?;

        // Targets are fetched concurrently; `join_all` keeps their order, which
        // decides which target's main APK and dex metadata win the merge.
        // Boxed: each download future is ~90kB, too large to hold inline.
        let results = join_all(DOWNLOAD_TARGETS.map(|(device_name, abi)| {
            Box::pin(async move {
                self.get_client_for_device(channel, device_name, &[abi])
                    .await?
                    .get_download_info(package_name, Some(version_code))
                    .await
            })
        }))
        .await;

        let mut download_infos = Vec::new();
        let mut errors = Vec::new();
        for ((_, abi), result) in DOWNLOAD_TARGETS.into_iter().zip(results) {
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
            Ok(DownloadLookup::Found(Box::new((
                details,
                merge_download_infos(download_infos),
            ))))
        }
    }
}

#[must_use]
fn merge_download_infos(download_infos: Vec<DownloadInfo>) -> DownloadInfo {
    let mut merged = DownloadInfo::default();
    let mut split_names = HashSet::new();
    let mut additional_filenames = HashSet::new();

    for info in download_infos {
        merged.main_apk_url = merged.main_apk_url.or(info.main_apk_url);
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
    let (name, url) = &item;
    let key = name.as_ref().or(url.as_ref()).cloned().unwrap_or_default();
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
}
