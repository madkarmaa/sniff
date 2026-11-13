//! A library for interacting with the Google Play API.
//!
//! # Getting Started
//!
//! To interact with the API, first you'll have to obtain an OAuth token by visiting the Google
//! [embedded setup page](https://accounts.google.com/EmbeddedSetup/identifier?flowName=EmbeddedSetupAndroid)
//! and opening the browser debugging console, logging in, and looking for the `oauth_token` cookie
//! being set on your browser.  It will be present in the last requests being made and start with
//! `oauth2_4/`.  Copy this value.  It works only once, to obtain the `aas_token`, which later
//! logins use.  To obtain this token:
//!
//! ```rust
//! use gpapi::Gpapi;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!     let mut api = Gpapi::new("px_9_fold", &email)?;
//!     api.request_aas_token(oauth_token).await?;
//!     println!("{:?}", api.get_aas_token());
//!     Ok(())
//! }
//! ```
//!
//! Now, you can begin interacting with the API by initializing it setting the `aas_token` and
//! logging in.
//!
//! ```rust
//! use gpapi::Gpapi;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!     let mut api = Gpapi::new("px_7a", &email)?;
//!     api.set_aas_token(aas_token);
//!     api.login().await?;
//!     // do something
//!     Ok(())
//! }
//! ```
//!
//! From here, you can get package details or get the info to download a package.
//!
//! ```rust
//! # use gpapi::Gpapi;
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! # let mut api = Gpapi::new("px_7a", &email)?;
//! # api.set_aas_token(aas_token);
//! # api.login().await?;
//! let details = api.details("com.instagram.android").await;
//! println!("{:?}", details);
//!
//! let download_info = api.get_download_info("com.instagram.android", None).await;
//! println!("{:?}", download_info);
//! # Ok(())
//! # }
//! ```

// On wasm, reqwest futures wrap JavaScript promises, which are never `Send`.
#![cfg_attr(target_arch = "wasm32", allow(clippy::future_not_send))]

mod auth;
mod auth_token;
mod checkin;
mod consts;
mod delivery;
mod details;
mod devices;
pub mod error;
mod headers;
mod resume;
mod session;
mod transport;

pub use delivery::{DownloadInfo, NamedDownload};
pub use resume::SessionState;

use googleplay_protobuf::{AndroidCheckinProto, DeviceConfigurationProto};
use prost::Message;
use std::collections::HashMap;
use std::error::Error;

use crate::devices::DEVICES;
use crate::error::Error as GpapiError;

use bincode::{Decode, Encode};
include!("device_properties.rs");

#[derive(Debug, Clone)]
pub struct Gpapi {
    locale: String,
    timezone: String,
    device_properties: DeviceProperties,
    email: String,
    aas_token: Option<String>,
    auth_token: Option<String>,
    device_config_token: Option<String>,
    device_checkin_consistency_token: Option<String>,
    tos_token: Option<String>,
    dfe_cookie: Option<String>,
    gsf_id: Option<i64>,
    client: reqwest::Client,
}

impl Gpapi {
    /// Returns a `Gpapi` struct.
    ///
    /// # Errors
    ///
    /// Returns an error if the embedded device database cannot be decoded,
    /// if `device_codename` is unknown, or if the stored device properties
    /// cannot be decoded.
    pub fn new(
        device_codename: impl Into<String>,
        email: impl Into<String>,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let codename: String = device_codename.into();
        let devices = DEVICES
            .as_ref()
            .map_err(|e| -> Box<dyn Error + Send + Sync> {
                Box::new(GpapiError::from(format!(
                    "failed to decode device database: {e}"
                )))
            })?;
        let encoded = devices
            .get(&codename)
            .ok_or_else(|| -> Box<dyn Error + Send + Sync> {
                Box::new(GpapiError::from(format!(
                    "invalid device codename: {codename}"
                )))
            })?;
        let device_properties = encoded
            .decode()
            .map_err(|e| -> Box<dyn Error + Send + Sync> {
                Box::new(GpapiError::from(format!(
                    "failed to decode device properties: {e}"
                )))
            })?;
        Ok(Self {
            locale: String::from("en_US"),
            timezone: String::from("UTC"),
            device_properties,
            email: email.into(),
            aas_token: None,
            auth_token: None,
            device_config_token: None,
            device_checkin_consistency_token: None,
            tos_token: None,
            dfe_cookie: None,
            gsf_id: None,
            client: reqwest::Client::new(),
        })
    }

    /// Set the locale
    pub fn set_locale<S: Into<String>>(&mut self, locale: S) {
        self.locale = locale.into();
    }

    /// Set the time zone
    pub fn set_timezone<S: Into<String>>(&mut self, timezone: S) {
        self.timezone = timezone.into();
    }

    /// Override the ABIs advertised to Google Play for this device.
    ///
    /// This must be called before `login` so the overridden device
    /// configuration is used during check-in and device-config upload.
    pub fn set_supported_abis<I, S>(&mut self, supported_abis: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.device_properties.device_configuration.native_platform =
            supported_abis.into_iter().map(Into::into).collect();
    }

    /// Set the aas token. This can be requested via `request_aas_token`, and is required for most
    /// other actions.
    pub fn set_aas_token<S: Into<String>>(&mut self, aas_token: S) {
        self.aas_token = Some(aas_token.into());
    }

    /// Get the aas token that has been previously set by either `request_aas_token` or
    /// `set_aas_token`.
    #[must_use]
    pub fn get_aas_token(&self) -> Option<&str> {
        self.aas_token.as_deref()
    }
}

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;
