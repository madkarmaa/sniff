use googleplay_protobuf::DetailsResponse;
use gpapi::{DownloadInfo, Gpapi};
use std::str::FromStr;

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
    /// Whether `details`, fetched with this channel's account, offer this channel.
    ///
    /// Beta and alpha builds are only served to accounts enrolled in the app's
    /// testing program, which Google Play reports in the app details.
    #[must_use]
    pub fn is_available_in(self, details: &DetailsResponse) -> bool {
        match self {
            Self::Stable => true,
            Self::Beta | Self::Alpha => details
                .item
                .as_ref()
                .and_then(|item| item.details.as_ref())
                .and_then(|details| details.app_details.as_ref())
                .and_then(|app_details| app_details.testing_program_info.as_ref())
                .and_then(|info| info.subscribed)
                .unwrap_or(false),
        }
    }
}

#[derive(Clone)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use googleplay_protobuf::{AppDetails, DocumentDetails, Item, TestingProgramInfo};

    fn details(subscribed: Option<bool>) -> DetailsResponse {
        DetailsResponse {
            item: Some(Item {
                details: Some(DocumentDetails {
                    app_details: Some(AppDetails {
                        testing_program_info: subscribed.map(|subscribed| TestingProgramInfo {
                            subscribed: Some(subscribed),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn testing_channels_require_testing_program_enrollment() {
        for channel in [Channel::Beta, Channel::Alpha] {
            assert!(channel.is_available_in(&details(Some(true))));
            assert!(!channel.is_available_in(&details(Some(false))));
            assert!(!channel.is_available_in(&details(None)));
        }
        assert!(Channel::Stable.is_available_in(&details(None)));
    }
}
