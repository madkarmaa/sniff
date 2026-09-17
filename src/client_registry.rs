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
    pub fn new(env: Env) -> Self {
        Self {
            clients: HashMap::new(),
            initialized: HashMap::new(),
            env,
        }
    }

    pub async fn get_client(&mut self, channel: Channel) -> Result<&GooglePlayClient, String> {
        let device_name = self.env.var("DEVICE_NAME").unwrap().to_string();
        self.get_client_for_device(channel, &device_name, &[]).await
    }

    async fn get_client_for_device(
        &mut self,
        channel: Channel,
        device_name: &str,
        supported_abis: &[&str],
    ) -> Result<&GooglePlayClient, String> {
        let supported_abis = supported_abis
            .iter()
            .map(|abi| (*abi).to_string())
            .collect::<Vec<_>>();
        let key = (channel, device_name.to_string(), supported_abis.clone());

        if !self.clients.contains_key(&key) {
            let (email, aas_token) = match channel {
                Channel::Stable => (
                    self.env.var("STABLE_EMAIL").unwrap().to_string(),
                    self.env.var("STABLE_AAS_TOKEN").unwrap().to_string(),
                ),
                Channel::Beta => (
                    self.env.var("BETA_EMAIL").unwrap().to_string(),
                    self.env.var("BETA_AAS_TOKEN").unwrap().to_string(),
                ),
                Channel::Alpha => (
                    self.env.var("ALPHA_EMAIL").unwrap().to_string(),
                    self.env.var("ALPHA_AAS_TOKEN").unwrap().to_string(),
                ),
            };

            let client = if supported_abis.is_empty() {
                GooglePlayClient::new(device_name, &email, &aas_token, channel)
            } else {
                GooglePlayClient::new_for_abis(
                    device_name,
                    &supported_abis,
                    &email,
                    &aas_token,
                    channel,
                )
            };
            self.clients.insert(key.clone(), client);
            self.initialized.insert(key.clone(), false);
        }

        if !self.initialized.get(&key).unwrap_or(&false) {
            let client = self.clients.get_mut(&key).unwrap();
            client.initialize().await?;
            self.initialized.insert(key.clone(), true);
        }

        Ok(self.clients.get(&key).unwrap())
    }

    pub async fn get_details_with_fallback(
        &mut self,
        package_name: &str,
        channel: Channel,
    ) -> Result<Option<(Channel, googleplay_protobuf::DetailsResponse)>, String> {
        if !channel.is_available_for_package(package_name) {
            return Err(format!(
                "Channel '{}' is not available for package '{}'",
                channel, package_name
            ));
        }

        let client = self.get_client(channel).await?;
        match client.get_details(package_name).await {
            Ok(Some(response)) => Ok(Some((channel, response))),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

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
                return Err(format!("App '{}' not found", package_name));
            }
            Err(e) => {
                console_log!("Error fetching {} for stable channel: {}", package_name, e);
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
                    console_log!("Error fetching {} for beta channel: {}", package_name, e);
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
                    console_log!("Error fetching {} for alpha channel: {}", package_name, e);
                }
                _ => {}
            }
        }

        Ok(results)
    }

    pub async fn get_download_info(
        &mut self,
        package_name: &str,
        channel: Channel,
        version_code: Option<i64>,
    ) -> Result<Option<(Channel, DownloadInfo)>, String> {
        if !channel.is_available_for_package(package_name) {
            return Err(format!(
                "Channel '{}' is not available for package '{}'",
                channel, package_name
            ));
        }

        let mut download_infos = Vec::new();
        let mut errors = Vec::new();

        for (device_name, abi) in DOWNLOAD_TARGETS {
            let result = match self
                .get_client_for_device(channel, device_name, &[abi])
                .await
            {
                Ok(client) => client.get_download_info(package_name, version_code).await,
                Err(error) => Err(error),
            };

            match result {
                Ok(download_info) => download_infos.push(download_info),
                Err(error) => {
                    console_log!(
                        "Error fetching {} download for {}: {}",
                        abi,
                        package_name,
                        error
                    );
                    errors.push(format!("{}: {}", abi, error));
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
