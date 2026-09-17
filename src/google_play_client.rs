use googleplay_protobuf::DetailsResponse;
use gpapi::{DownloadInfo, Gpapi};
use std::str::FromStr;

const BETA_ALPHA_PACKAGES: &[&str] = &["com.discord"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    Stable,
    Beta,
    Alpha,
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stable => write!(f, "stable"),
            Self::Beta => write!(f, "beta"),
            Self::Alpha => write!(f, "alpha"),
        }
    }
}

impl FromStr for Channel {
    type Err = String;

    /// Parse a channel name (case-insensitive).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "stable" => Ok(Self::Stable),
            "beta" => Ok(Self::Beta),
            "alpha" => Ok(Self::Alpha),
            _ => Err(format!("Invalid Channel: {s}")),
        }
    }
}

impl Channel {
    #[must_use]
    pub fn is_available_for_package(self, package_name: &str) -> bool {
        match self {
            Self::Stable => true,
            Self::Beta | Self::Alpha => BETA_ALPHA_PACKAGES.contains(&package_name),
        }
    }
}

pub struct GooglePlayClient {
    client: Gpapi,
    channel: Channel,
}

impl GooglePlayClient {
    /// Create a client, optionally overriding the advertised ABIs.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying `Gpapi` client cannot be created
    /// for `device_name`.
    pub fn new_for_abis(
        device_name: &str,
        supported_abis: &[String],
        email: &str,
        aas_token: &str,
        channel: Channel,
    ) -> Result<Self, String> {
        let mut client =
            Gpapi::new(device_name, email).map_err(|e| format!("failed to create client: {e}"))?;
        if !supported_abis.is_empty() {
            client.set_supported_abis(supported_abis.iter().cloned());
        }
        client.set_aas_token(aas_token);

        Ok(Self { client, channel })
    }

    /// Log in to Google Play.
    ///
    /// # Errors
    ///
    /// Returns an error if login fails for this client's channel.
    pub async fn initialize(&mut self) -> Result<(), String> {
        let channel = self.channel;
        self.client
            .login()
            .await
            .map_err(|e| format!("Login error for {channel} channel: {e:?}"))
    }

    /// Fetch app details.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails for this client's channel.
    pub async fn get_details(&self, package_name: &str) -> Result<Option<DetailsResponse>, String> {
        let channel = self.channel;
        self.client
            .details(package_name)
            .await
            .map_err(|e| format!("API error for {channel} channel: {e:?}"))
    }

    /// Fetch download info.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails for this client's channel.
    pub async fn get_download_info(
        &self,
        package_name: &str,
        version_code: Option<i64>,
    ) -> Result<DownloadInfo, String> {
        let channel = self.channel;
        // Boxed: the download future is ~90kB, too large to hold inline.
        Box::pin(self.client.get_download_info(package_name, version_code))
            .await
            .map_err(|e| format!("API error for {channel} channel: {e:?}"))
    }
}
