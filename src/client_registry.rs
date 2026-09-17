use gpapi::DownloadInfo;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use worker::{console_log, Env};

use crate::google_play_client::{Channel, GooglePlayClient};

type ClientKey = (Channel, String, Vec<String>);

const DOWNLOAD_TARGETS: [(&str, &str); 4] = [
    ("px_9_fold", "arm64-v8a"),
    ("sm_a13_5g", "armeabi-v7a"),
    ("google_kiwi_x86_64", "x86"),
    ("google_kiwi_x86_64", "x86_64"),
];

pub struct ClientRegistry {
    clients: HashMap<ClientKey, GooglePlayClient>,
    initialized: HashMap<ClientKey, bool>,
    env: Env,
}

impl ClientRegistry {
    #[must_use]
    pub fn new(env: Env) -> Self {
        Self {
            clients: HashMap::new(),
            initialized: HashMap::new(),
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

        if !self.initialized.get(&key).copied().unwrap_or(false) {
            let client = self
                .clients
                .get_mut(&key)
                .ok_or_else(|| format!("client missing for channel {channel}"))?;
            client.initialize().await?;
            self.initialized.insert(key.clone(), true);
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

        if Channel::Beta.is_available_for_package(package_name) {
            match self
                .get_client(Channel::Beta)
                .await?
                .get_details(package_name)
                .await
            {
                Ok(Some(response)) => {
                    results.insert(Channel::Beta, response);
                }
                Err(e) => {
                    console_log!("Error fetching {package_name} for beta channel: {e}");
                }
                _ => {}
            }
        }

        if Channel::Alpha.is_available_for_package(package_name) {
            match self
                .get_client(Channel::Alpha)
                .await?
                .get_details(package_name)
                .await
            {
                Ok(Some(response)) => {
                    results.insert(Channel::Alpha, response);
                }
                Err(e) => {
                    console_log!("Error fetching {package_name} for alpha channel: {e}");
                }
                _ => {}
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

pub async fn create_registry(env: Env) -> SharedClientRegistry {
    let registry = ClientRegistry::new(env);
    Arc::new(Mutex::new(registry))
}
