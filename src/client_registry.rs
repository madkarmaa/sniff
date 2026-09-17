use gpapi::DownloadInfo;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;
use worker::{console_log, Date, Env};

use crate::google_play_client::{Channel, GooglePlayClient};

type ClientKey = (Channel, String, Vec<String>);

const DOWNLOAD_TARGETS: [(&str, &str); 4] = [
    ("px_9_fold", "arm64-v8a"),
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

pub struct ClientRegistry {
    clients: HashMap<ClientKey, GooglePlayClient>,
    initialized: HashMap<ClientKey, bool>,
    logged_in_at_ms: HashMap<ClientKey, u64>,
    env: Env,
}

impl ClientRegistry {
    #[must_use]
    pub fn new(env: Env) -> Self {
        Self {
            clients: HashMap::new(),
            initialized: HashMap::new(),
            logged_in_at_ms: HashMap::new(),
            env,
        }
    }

    /// Get a client for `channel` using the default device.
    ///
    /// # Errors
    ///
    /// Returns an error if required environment configuration is missing or
    /// if the client cannot be created or initialized.
    pub async fn get_client(&mut self, channel: Channel) -> Result<&GooglePlayClient, String> {
        let device_name = self
            .env
            .var("DEVICE_NAME")
            .map_err(|e| format!("missing DEVICE_NAME env: {e:?}"))?
            .to_string();
        self.get_client_for_device(channel, &device_name, &[]).await
    }

    async fn get_client_for_device(
        &mut self,
        channel: Channel,
        device_name: &str,
        supported_abis: &[&str],
    ) -> Result<&GooglePlayClient, String> {
        let supported_abis: Vec<String> =
            supported_abis.iter().copied().map(String::from).collect();
        let key = (channel, device_name.to_string(), supported_abis.clone());

        if !self.clients.contains_key(&key) {
            let (email, aas_token) = match channel {
                Channel::Stable => (
                    self.env
                        .var("STABLE_EMAIL")
                        .map_err(|e| format!("missing STABLE_EMAIL env: {e:?}"))?
                        .to_string(),
                    self.env
                        .var("STABLE_AAS_TOKEN")
                        .map_err(|e| format!("missing STABLE_AAS_TOKEN env: {e:?}"))?
                        .to_string(),
                ),
                Channel::Beta => (
                    self.env
                        .var("BETA_EMAIL")
                        .map_err(|e| format!("missing BETA_EMAIL env: {e:?}"))?
                        .to_string(),
                    self.env
                        .var("BETA_AAS_TOKEN")
                        .map_err(|e| format!("missing BETA_AAS_TOKEN env: {e:?}"))?
                        .to_string(),
                ),
                Channel::Alpha => (
                    self.env
                        .var("ALPHA_EMAIL")
                        .map_err(|e| format!("missing ALPHA_EMAIL env: {e:?}"))?
                        .to_string(),
                    self.env
                        .var("ALPHA_AAS_TOKEN")
                        .map_err(|e| format!("missing ALPHA_AAS_TOKEN env: {e:?}"))?
                        .to_string(),
                ),
            };

            let client = if supported_abis.is_empty() {
                GooglePlayClient::new(device_name, &email, &aas_token, channel)?
            } else {
                GooglePlayClient::new_for_abis(
                    device_name,
                    &supported_abis,
                    &email,
                    &aas_token,
                    channel,
                )?
            };
            self.clients.insert(key.clone(), client);
            self.initialized.insert(key.clone(), false);
        }

        let fresh = self.initialized.get(&key).copied().unwrap_or(false)
            && self.logged_in_at_ms.get(&key).is_some_and(|logged_in_at_ms| {
                Date::now()
                    .as_millis()
                    .saturating_sub(*logged_in_at_ms)
                    < SESSION_TTL_MS
            });

        if !fresh {
            let client = self
                .clients
                .get_mut(&key)
                .ok_or_else(|| format!("client missing for channel {channel}"))?;
            client.initialize().await?;
            self.initialized.insert(key.clone(), true);
            self.logged_in_at_ms
                .insert(key.clone(), Date::now().as_millis());
        }

        self.clients
            .get(&key)
            .ok_or_else(|| format!("client missing for channel {channel}"))
    }

    /// Get details for `package_name` on `channel`.
    ///
    /// # Errors
    ///
    /// Returns an error if the channel is unavailable for the package or if
    /// the underlying client request fails.
    pub async fn get_details_with_fallback(
        &mut self,
        package_name: &str,
        channel: Channel,
    ) -> Result<Option<(Channel, googleplay_protobuf::DetailsResponse)>, String> {
        if !channel.is_available_for_package(package_name) {
            return Err(format!(
                "Channel '{channel}' is not available for package '{package_name}'"
            ));
        }

        let client = self.get_client(channel).await?;
        match client.get_details(package_name).await {
            Ok(Some(response)) => Ok(Some((channel, response))),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Get details across all available channels.
    ///
    /// # Errors
    ///
    /// Returns an error if the stable channel lookup fails or the app is not
    /// found.
    pub async fn get_details_multi(
        &mut self,
        package_name: &str,
    ) -> Result<HashMap<Channel, googleplay_protobuf::DetailsResponse>, String> {
        let mut results = HashMap::new();

        match self
            .get_details_with_fallback(package_name, Channel::Stable)
            .await
        {
            Ok(Some((_, response))) => {
                results.insert(Channel::Stable, response);
            }
            Ok(None) => {
                return Err(format!("App '{package_name}' not found"));
            }
            Err(e) => {
                console_log!("Error fetching {package_name} for stable channel: {e}");
                return Err(e);
            }
        }

        // Beta/Alpha credentials are optional: if they are missing (or the
        // channel errors), skip the channel instead of failing the request.
        if Channel::Beta.is_available_for_package(package_name) {
            match self.get_client(Channel::Beta).await {
                Err(e) => {
                    console_log!("Skipping beta channel for {package_name}: {e}");
                }
                Ok(client) => match client.get_details(package_name).await {
                    Ok(Some(response)) => {
                        results.insert(Channel::Beta, response);
                    }
                    Err(e) => {
                        console_log!("Error fetching {package_name} for beta channel: {e}");
                    }
                    Ok(None) => {}
                },
            }
        }

        if Channel::Alpha.is_available_for_package(package_name) {
            match self.get_client(Channel::Alpha).await {
                Err(e) => {
                    console_log!("Skipping alpha channel for {package_name}: {e}");
                }
                Ok(client) => match client.get_details(package_name).await {
                    Ok(Some(response)) => {
                        results.insert(Channel::Alpha, response);
                    }
                    Err(e) => {
                        console_log!("Error fetching {package_name} for alpha channel: {e}");
                    }
                    Ok(None) => {}
                },
            }
        }

        Ok(results)
    }

    /// Get merged download info across download targets.
    ///
    /// # Errors
    ///
    /// Returns an error if the channel is unavailable for the package or if
    /// all per-ABI download attempts fail.
    pub async fn get_download_info(
        &mut self,
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
                Ok(client) => {
                    Box::pin(client.get_download_info(package_name, version_code)).await
                }
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
            let key = split
                .0
                .clone()
                .or_else(|| split.1.clone())
                .unwrap_or_default();
            if split_names.insert(key) {
                splits.push(split);
            }
        }

        for file in device_additional_files {
            let key = file
                .0
                .clone()
                .or_else(|| file.1.clone())
                .unwrap_or_default();
            if additional_filenames.insert(key) {
                additional_files.push(file);
            }
        }
    }

    (
        main_apk_url,
        splits,
        additional_files,
        dex_metadata_url,
    )
}

pub type SharedClientRegistry = Arc<Mutex<ClientRegistry>>;

/// Isolate-global registry. Clients log in once and are reused across
/// requests instead of performing a fresh device check-in per request
/// (which reads as suspicious activity server-side).
static GLOBAL_REGISTRY: OnceLock<SharedClientRegistry> = OnceLock::new();

/// Return the shared registry, creating it from `env` on first use.
/// Bindings are identical for every request served by this worker version,
/// so the first request's `env` is representative.
#[must_use]
pub fn shared_registry(env: &Env) -> SharedClientRegistry {
    GLOBAL_REGISTRY
        .get_or_init(|| Arc::new(Mutex::new(ClientRegistry::new(env.clone()))))
        .clone()
}
