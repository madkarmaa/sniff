//! A library for interacting with the Google Play API.
//!
//! # Getting Started
//!
//! To interact with the API, first you'll have to obtain an OAuth token by visiting the Google
//! [embedded setup page](https://accounts.google.com/EmbeddedSetup/identifier?flowName=EmbeddedSetupAndroid)
//! and opening the browser debugging console, logging in, and looking for the `oauth_token` cookie
//! being set on your browser.  It will be present in the last requests being made and start with
//! `oauth2_4/`.  Copy this value.  It can only be used once, in order to obtain the `aas_token`,
//! which can be used subsequently.  To obtain this token:
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
//! From here, you can get package details, get the info to download a package, or use the library to download it.
//!
//! ```rust
//! # use gpapi::Gpapi;
//! # use std::path::Path;
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
//!
//! api.download("com.instagram.android", None, true, true, true, &Path::new("/tmp/testing"), None).await;
//! # Ok(())
//! # }
//! ```

mod consts;
pub mod error;

use bytes::Bytes;
use prost::Message;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Url;
use std::collections::HashMap;
use std::error::Error;
use std::io::Cursor;

use crate::error::{Error as GpapiError, ErrorKind as GpapiErrorKind};

use googleplay_protobuf::{
    AcceptTosResponse, AndroidCheckinProto, AndroidCheckinRequest, AndroidCheckinResponse,
    BulkDetailsRequest, BulkDetailsResponse, DetailsResponse, DeviceConfigurationProto,
    ResponseWrapper, UploadDeviceConfigRequest, UploadDeviceConfigResponse,
};

use bincode::{Decode, Encode};
include!("device_properties.rs");

static DEVICES_ENCODED: &[u8] = include_bytes!("device_properties.bin");

pub type MainAPKDownloadURL = Option<String>;
pub type SplitsDownloadInfo = Vec<(Option<String>, Option<String>)>;
pub type AdditionalFilesDownloadInfo = Vec<(Option<String>, Option<String>)>;
pub type DexMetadataURL = Option<String>;
pub type DownloadInfo = (
    MainAPKDownloadURL,
    SplitsDownloadInfo,
    AdditionalFilesDownloadInfo,
    DexMetadataURL,
);

#[derive(Debug)]
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
    client: Box<reqwest::Client>,
}

impl Gpapi {
    /// Returns a `Gpapi` struct.
    ///
    /// # Errors
    ///
    /// Returns an error if the embedded device database cannot be decoded,
    /// if `device_codename` is unknown, or if the stored device properties
    /// cannot be decoded.
    pub fn new<S: Into<String>>(
        device_codename: S,
        email: S,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let codename: String = device_codename.into();
        let (mut devices, _) = bincode::borrow_decode_from_slice::<
            HashMap<String, EncodedDeviceProperties>,
            bincode::config::Configuration,
        >(DEVICES_ENCODED, bincode::config::standard())
        .map_err(|e| -> Box<dyn Error + Send + Sync> {
            Box::new(GpapiError::from(format!(
                "failed to decode device database: {e}"
            )))
        })?;
        let encoded =
            devices
                .remove(&codename)
                .ok_or_else(|| -> Box<dyn Error + Send + Sync> {
                    Box::new(GpapiError::from(format!(
                        "invalid device codename: {codename}"
                    )))
                })?;
        let device_properties =
            encoded
                .into_decoded()
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
            client: Box::new(reqwest::Client::new()),
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

    /// Request and set the aas token given an oauth token and the associated email.
    ///
    /// # Arguments
    ///
    /// * `oauth_token` - An oauth token you previously retrieved separately
    ///
    /// # Errors
    ///
    /// Returns an error if the authentication request fails or the response
    /// does not contain a token.
    pub async fn request_aas_token<S: Into<String>>(
        &mut self,
        oauth_token: S,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let oauth_token: String = oauth_token.into();
        let auth_req = AuthRequest::new(&self.email, &oauth_token);
        let mut resp = self.request_aas_token_helper(&auth_req).await?;
        self.aas_token = Some(
            resp.remove("token")
                .ok_or_else(|| Box::new(GpapiError::new(GpapiErrorKind::Authentication)))?,
        );
        Ok(())
    }

    async fn request_aas_token_helper(
        &self,
        auth_req: &AuthRequest,
    ) -> Result<HashMap<String, String>, Box<dyn Error + Send + Sync>> {
        let form_body = form_post(&auth_req.params);

        let mut headers = HashMap::new();
        headers.insert(
            "user-agent",
            String::from(consts::defaults::DEFAULT_AUTH_USER_AGENT),
        );
        headers.insert(
            "content-type",
            String::from("application/x-www-form-urlencoded"),
        );
        headers.insert("app", String::from("com.google.android.gms"));

        let body_bytes = self
            .execute_request_helper("auth", None, Some(&form_body.into_bytes()), headers, false)
            .await?;

        let reply = parse_form_reply(std::str::from_utf8(&body_bytes)?);
        Ok(reply)
    }

    /// Get the aas token that has been previously set by either `request_aas_token` or
    /// `set_aas_token`.
    #[must_use]
    pub fn get_aas_token(&self) -> Option<&str> {
        self.aas_token.as_deref()
    }

    /// Log in to Google's Play Store API.  This is required for most other actions. The aas token
    /// has to be set via `request_aas_token` or `set_aas_token` first.
    ///
    /// Terms of service presented by fresh sessions are accepted inline by
    /// `toc`; if the gate is still up afterwards, the check is retried once
    /// in case the acceptance only takes effect on the next call.
    ///
    /// # Errors
    ///
    /// Returns an error if check-in, device-config upload, authentication,
    /// or the terms-of-service check fails, if `acceptTos` is not
    /// acknowledged, or if no device-config token is returned.
    pub async fn login(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.checkin().await?;
        if let Some(upload_device_config_token) = self.upload_device_config().await? {
            let token = upload_device_config_token
                .upload_device_config_token
                .ok_or_else(|| Box::new(GpapiError::new(GpapiErrorKind::InvalidResponse)))?;
            self.device_config_token = Some(token);
            self.request_auth_token().await?;
            // `toc` accepts presented terms inline; retry once in case the
            // acceptance only takes effect on the next call. Both calls are
            // boxed: `toc` now contains the whole accept handshake and would
            // otherwise blow past the future-size limit.
            match Box::pin(self.toc()).await {
                Err(e)
                    if e.downcast_ref::<GpapiError>().is_some_and(|api_error| {
                        matches!(api_error.kind(), GpapiErrorKind::TermsOfService)
                    }) =>
                {
                    Box::pin(self.toc()).await
                }
                other => other,
            }
        } else {
            Err("No device config token".into())
        }
    }

    /// Retrieve the download URL(s) and names for a package, given a package ID and optional
    /// version code.
    ///
    /// # Arguments
    ///
    /// * `pkg_name` - A string type specifying the package's app ID, e.g. `com.instagram.android`
    /// * `version_code` - An optinal version code, given in i32.  If omitted, the latest version will
    ///   be used
    ///
    /// # Returns
    ///
    /// * An Option<String> to the full APK download URL, followed by a Vec<(Option<String>,
    ///   Option<String>)> which corresponds to a list of download URLs and names for the split APK,
    ///   then followed by another Vec<(Option<String>, Option<String>)> which corresponds to the
    ///   download URLs and filenames for additional files and finally an Option<String> that
    ///   contains the URL for the dexmetadata file.
    ///
    /// # Errors
    ///
    /// Returns an error if login is required, if the latest version cannot be
    /// determined, or if the purchase/delivery flow does not yield download
    /// URLs.
    pub async fn get_download_info<S: Into<String>>(
        &self,
        pkg_name: S,
        version_code: Option<i64>,
    ) -> Result<DownloadInfo, Box<dyn Error + Send + Sync>> {
        let pkg_name: String = pkg_name.into();
        if self.auth_token.is_none() {
            return Err(Box::new(GpapiError::new(GpapiErrorKind::LoginRequired)));
        }
        let version_code = match version_code {
            Some(v) => v,
            None => self.get_latest_version_for_pkg_name(&pkg_name).await?,
        };
        let resp = {
            let version_code_string = version_code.to_string();
            let mut params = HashMap::new();
            params.insert("ot", String::from("1"));
            params.insert("doc", String::from(&pkg_name));
            params.insert("vc", version_code_string);

            let mut headers = self.get_default_headers()?;
            headers.insert("content-length", String::from("0"));

            self.execute_request("purchase", Some(params), Some(&[]), headers)
                .await?
        };
        if let Some(payload) = resp.payload
            && let Some(buy_response) = payload.buy_response
            && let Some(delivery_token) = buy_response.encoded_delivery_token
        {
            return self
                .delivery(&pkg_name, Some(version_code), &delivery_token)
                .await;
        }
        Err(Box::new(GpapiError::new(GpapiErrorKind::InvalidApp)))
    }

    async fn delivery<S: Into<String>>(
        &self,
        pkg_name: S,
        version_code: Option<i64>,
        delivery_token: S,
    ) -> Result<DownloadInfo, Box<dyn Error + Send + Sync>> {
        let pkg_name: String = pkg_name.into();
        let delivery_token: String = delivery_token.into();
        if self.auth_token.is_none() {
            return Err(Box::new(GpapiError::new(GpapiErrorKind::LoginRequired)));
        }
        let version_code = match version_code {
            Some(v) => v,
            None => self.get_latest_version_for_pkg_name(&pkg_name).await?,
        };
        let resp = {
            let version_code_string = version_code.to_string();
            let mut req = HashMap::new();
            req.insert("ot", String::from("1"));
            req.insert("doc", pkg_name.clone());
            req.insert("vc", version_code_string);
            req.insert("dtok", delivery_token);
            self.execute_request("delivery", Some(req), None, self.get_default_headers()?)
                .await?
        };
        if let Some(payload) = resp.payload
            && let Some(delivery_response) = payload.delivery_response
            && let Some(app_delivery_data) = delivery_response.app_delivery_data
        {
            let mut splits = Vec::new();
            for app_split_delivery_data in app_delivery_data.split_delivery_data {
                splits.push((
                    app_split_delivery_data.name,
                    app_split_delivery_data.download_url,
                ));
            }
            let mut additional_files: Vec<(Option<String>, Option<String>)> = Vec::new();
            for additional_file in app_delivery_data.additional_file {
                if let Some(file_type) = additional_file.file_type
                    && let Some(version_code) = additional_file.version_code
                {
                    let main_patch = match file_type {
                        0 => "main",
                        _ => "patch",
                    };
                    let filename =
                        format!("{main_patch}.{version_code}.{pkg_name}.obb");
                    additional_files
                        .push((Some(filename), additional_file.download_url));
                }
            }
            let dex_metadata_url = app_delivery_data
                .dex_metadata
                .and_then(|dex_metadata| dex_metadata.download_url);
            return Ok((
                app_delivery_data.download_url,
                splits,
                additional_files,
                dex_metadata_url,
            ));
        }
        Err(Box::new(GpapiError::new(GpapiErrorKind::InvalidApp)))
    }

    async fn get_latest_version_for_pkg_name(
        &self,
        pkg_name: &str,
    ) -> Result<i64, Box<dyn Error + Send + Sync>> {
        if let Some(details) = self.details(pkg_name).await?
            && let Some(item) = details.item
            && let Some(details) = item.details
            && let Some(app_details) = details.app_details
            && let Some(version_code) = app_details.version_code
        {
            return Ok(version_code);
        }
        Err(Box::new(GpapiError::new(GpapiErrorKind::InvalidApp)))
    }

    /// Play Store package detail request (provides more detail than bulk requests).
    ///
    /// # Arguments
    ///
    /// * `pkg_name` - A string type specifying the package's app ID, e.g. `com.instagram.android`
    ///
    /// # Errors
    ///
    /// Returns an error if login is required or if the details request fails.
    pub async fn details<S: Into<String>>(
        &self,
        pkg_name: S,
    ) -> Result<Option<DetailsResponse>, Box<dyn Error + Send + Sync>> {
        if self.auth_token.is_none() {
            return Err(Box::new(GpapiError::new(GpapiErrorKind::LoginRequired)));
        }
        let mut form_params = HashMap::new();
        form_params.insert("doc", pkg_name.into());

        let headers = self.get_default_headers()?;

        let resp = self
            .execute_request("details", Some(form_params), None, headers)
            .await?;

        Ok(resp.payload.and_then(|payload| payload.details_response))
    }

    /// Play Store bulk detail request for multiple apps.
    ///
    /// # Arguments
    ///
    /// * `pkg_names` - An array of string types specifying package app IDs
    ///
    /// # Errors
    ///
    /// Returns an error if login is required or if the bulk-details request
    /// fails.
    pub async fn bulk_details(
        &self,
        pkg_names: &[&str],
    ) -> Result<Option<BulkDetailsResponse>, Box<dyn Error + Send + Sync>> {
        if self.auth_token.is_none() {
            return Err(Box::new(GpapiError::new(GpapiErrorKind::LoginRequired)));
        }
        let mut req = BulkDetailsRequest {
            doc_id: pkg_names.iter().copied().map(String::from).collect(),
            include_child_docs: Some(false),
            ..Default::default()
        };
        req.doc_id = pkg_names.iter().copied().map(String::from).collect();
        req.include_child_docs = Some(false);
        let mut bytes = Vec::with_capacity(req.encoded_len());
        req.encode(&mut bytes)?;
        let resp = self
            .execute_request(
                "bulkDetails",
                None,
                Some(&bytes),
                self.get_default_headers()?,
            )
            .await?;
        Ok(resp
            .payload
            .and_then(|payload| payload.bulk_details_response))
    }

    async fn checkin(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let checkin = self.device_properties.android_checkin.clone();

        let build_device = checkin
            .build
            .as_ref()
            .ok_or_else(|| Box::new(GpapiError::new(GpapiErrorKind::InvalidResponse)))?
            .device
            .as_ref()
            .ok_or_else(|| Box::new(GpapiError::new(GpapiErrorKind::InvalidResponse)))?
            .clone();

        let req = AndroidCheckinRequest {
            id: Some(0),
            checkin: Some(checkin),
            locale: Some(self.locale.clone()),
            time_zone: Some(self.timezone.clone()),
            version: Some(3),
            device_configuration: Some(self.device_properties.device_configuration.clone()),
            fragment: Some(0),
            ..Default::default()
        };
        let mut bytes = Vec::with_capacity(req.encoded_len());
        req.encode(&mut bytes)?;

        let build_id = self
            .device_properties
            .extra_info
            .get("Build.ID")
            .ok_or_else(|| Box::new(GpapiError::new(GpapiErrorKind::InvalidResponse)))?
            .clone();
        let mut headers = HashMap::new();
        self.append_auth_headers(&mut headers, build_device, build_id);

        let resp = self.execute_checkin_request(&bytes, headers).await?;
        self.device_checkin_consistency_token = resp.device_checkin_consistency_token;
        self.gsf_id = resp.android_id.map(u64::cast_signed);
        Ok(())
    }

    async fn execute_checkin_request(
        &self,
        msg: &[u8],
        mut auth_headers: HashMap<&str, String>,
    ) -> Result<AndroidCheckinResponse, Box<dyn Error + Send + Sync>> {
        auth_headers.insert("content-type", String::from("application/x-protobuf"));
        auth_headers.insert("host", String::from("android.clients.google.com"));
        let bytes = self
            .execute_request_helper("checkin", None, Some(msg), auth_headers, false)
            .await?;
        let resp = AndroidCheckinResponse::decode(&mut Cursor::new(bytes))?;
        Ok(resp)
    }

    fn get_default_headers(&self) -> Result<HashMap<&str, String>, Box<dyn Error + Send + Sync>> {
        let mut headers = HashMap::new();
        self.append_default_headers(&mut headers)?;
        Ok(headers)
    }

    fn append_default_headers(
        &self,
        headers: &mut HashMap<&str, String>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Some(auth_token) = &self.auth_token {
            headers.insert("Authorization", format!("Bearer {auth_token}"));
        }

        let build = self
            .device_properties
            .android_checkin
            .clone()
            .build
            .ok_or_else(|| Box::new(GpapiError::new(GpapiErrorKind::InvalidResponse)))?;
        let device_configuration = self.device_properties.device_configuration.clone();

        let invalid = || Box::new(GpapiError::new(GpapiErrorKind::InvalidResponse));
        let vending_version_string = self
            .device_properties
            .extra_info
            .get("Vending.versionString")
            .ok_or_else(invalid)?;
        let vending_version = self
            .device_properties
            .extra_info
            .get("Vending.version")
            .ok_or_else(invalid)?;
        let sdk_version = build.sdk_version.as_ref().ok_or_else(invalid)?.to_string();
        let device = build.device.as_ref().ok_or_else(invalid)?;
        let product = build.product.as_ref().ok_or_else(invalid)?;
        let build_product = build.build_product.as_ref().ok_or_else(invalid)?;
        let release = self
            .device_properties
            .extra_info
            .get("Build.VERSION.RELEASE")
            .ok_or_else(invalid)?;
        let model = build.model.as_ref().ok_or_else(invalid)?;
        let build_id = self
            .device_properties
            .extra_info
            .get("Build.ID")
            .ok_or_else(invalid)?;

        let build_configuration = BuildConfiguration::new(
            vending_version_string,
            vending_version,
            &sdk_version,
            device,
            product,
            build_product,
            release,
            model,
            build_id,
            &device_configuration.native_platform.join(";"),
        );

        headers.insert("user-agent", build_configuration.user_agent());

        if let Some(gsf_id) = &self.gsf_id {
            headers.insert("X-DFE-Device-Id", format!("{gsf_id:x}"));
        }
        headers.insert("accept-language", self.locale.replace('_', "-"));
        headers.insert(
            "X-DFE-Encoded-Targets",
            String::from(consts::defaults::DEFAULT_DFE_TARGETS),
        );
        headers.insert(
            "X-DFE-Phenotype",
            String::from(consts::defaults::DEFAULT_DFE_PHENOTYPE),
        );
        headers.insert("X-DFE-Client-Id", String::from("am-android-google"));
        headers.insert("X-DFE-Network-Type", String::from("4"));
        headers.insert("X-DFE-Content-Filters", String::new());
        headers.insert("X-Limit-Ad-Tracking-Enabled", String::from("false"));
        headers.insert("X-Ad-Id", String::new());
        headers.insert("X-DFE-UserLanguages", self.locale.clone());
        headers.insert("X-DFE-Request-Params", String::from("timeoutMs=4000"));
        if let Some(device_checkin_consistency_token) = &self.device_checkin_consistency_token {
            headers.insert(
                "X-DFE-Device-Checkin-Consistency-Token",
                device_checkin_consistency_token.clone(),
            );
        }
        if let Some(device_config_token) = &self.device_config_token {
            headers.insert("X-DFE-Device-Config-Token", device_config_token.clone());
        }
        if let Some(dfe_cookie) = &self.dfe_cookie {
            headers.insert("X-DFE-Cookie", dfe_cookie.clone());
        }
        if let Some(mcc_mcn) = self.device_properties.extra_info.get("SimOperator") {
            headers.insert("X-DFE-MCCMCN", mcc_mcn.clone());
        }
        Ok(())
    }

    fn append_auth_headers<S: Into<String>>(
        &self,
        headers: &mut HashMap<&str, String>,
        build_device: S,
        build_id: S,
    ) {
        let build_device: String = build_device.into();
        let build_id: String = build_id.into();
        headers.insert(
            "app",
            String::from(consts::defaults::DEFAULT_ANDROID_VENDING),
        );
        headers.insert(
            "User-Agent",
            format!("GoogleAuth/1.4 ({build_device} {build_id})"),
        );
        if let Some(gsf_id) = self.gsf_id {
            headers.insert("device", format!("{gsf_id:x}"));
        }
    }

    fn append_default_auth_params(
        &self,
        params: &mut HashMap<&str, String>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Some(gsf_id) = self.gsf_id {
            params.insert("androidId", format!("{gsf_id:x}"));
        }

        let build = self
            .device_properties
            .android_checkin
            .clone()
            .build
            .ok_or_else(|| Box::new(GpapiError::new(GpapiErrorKind::InvalidResponse)))?;
        let invalid = || Box::new(GpapiError::new(GpapiErrorKind::InvalidResponse));
        params.insert(
            "sdk_version",
            build.sdk_version.as_ref().ok_or_else(invalid)?.to_string(),
        );
        params.insert("Email", self.email.clone());
        params.insert(
            "google_play_services_version",
            build
                .google_services
                .as_ref()
                .ok_or_else(invalid)?
                .to_string(),
        );
        params.insert(
            "device_country",
            String::from(consts::defaults::DEFAULT_COUNTRY_CODE).to_ascii_lowercase(),
        );
        params.insert(
            "lang",
            String::from(consts::defaults::DEFAULT_LANGUAGE).to_ascii_lowercase(),
        );
        params.insert(
            "callerSig",
            String::from(consts::defaults::DEFAULT_CALLER_SIG),
        );
        Ok(())
    }

    fn append_auth_params(
        &self,
        params: &mut HashMap<&str, String>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        params.insert("app", String::from("com.android.vending"));
        params.insert(
            "client_sig",
            String::from(consts::defaults::DEFAULT_CLIENT_SIG),
        );
        params.insert(
            "callerPkg",
            String::from(consts::defaults::DEFAULT_ANDROID_VENDING),
        );
        let token = self
            .aas_token
            .as_ref()
            .ok_or_else(|| Box::new(GpapiError::new(GpapiErrorKind::Authentication)))?
            .clone();
        params.insert("Token", token);
        params.insert("oauth2_foreground", String::from("1"));
        params.insert("token_request_options", String::from("CAA4AVAB"));
        params.insert("check_email", String::from("1"));
        params.insert("system_partition", String::from("1"));
        Ok(())
    }

    async fn upload_device_config(
        &self,
    ) -> Result<Option<UploadDeviceConfigResponse>, Box<dyn Error + Send + Sync>> {
        let req = UploadDeviceConfigRequest {
            device_configuration: Some(self.device_properties.device_configuration.clone()),
            ..Default::default()
        };
        let mut bytes = Vec::with_capacity(req.encoded_len());
        req.encode(&mut bytes)?;

        let mut headers = self.get_default_headers()?;
        headers.insert("content-type", String::from("application/x-protobuf"));

        let resp = self
            .execute_request("uploadDeviceConfig", None, Some(&bytes), headers)
            .await?;
        Ok(resp
            .payload
            .and_then(|payload| payload.upload_device_config_response))
    }

    async fn request_auth_token(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let form_params = {
            let mut params = HashMap::new();
            self.append_default_auth_params(&mut params)?;
            self.append_auth_params(&mut params)?;
            params.insert(
                "service",
                String::from("oauth2:https://www.googleapis.com/auth/googleplay"),
            );
            params
        };

        let headers = {
            let mut headers = HashMap::new();
            let build_device = self
                .device_properties
                .android_checkin
                .clone()
                .build
                .as_ref()
                .ok_or_else(|| Box::new(GpapiError::new(GpapiErrorKind::InvalidResponse)))?
                .device
                .as_ref()
                .ok_or_else(|| Box::new(GpapiError::new(GpapiErrorKind::InvalidResponse)))?
                .clone();
            let build_id = self
                .device_properties
                .extra_info
                .get("Build.ID")
                .ok_or_else(|| Box::new(GpapiError::new(GpapiErrorKind::InvalidResponse)))?
                .clone();
            self.append_auth_headers(&mut headers, build_device, build_id);
            headers.insert("content-length", String::from("0"));
            headers
        };

        let bytes = self
            .execute_request_helper("auth", Some(form_params), Some(&[]), headers, false)
            .await?;

        let reply = parse_form_reply(std::str::from_utf8(&bytes)?);
        let auth_token = reply.get("auth").cloned().ok_or_else(|| {
            let google_error = reply.get("error").cloned().unwrap_or_default();
            Box::new(GpapiError::from(format!(
                "authentication failed: {google_error}"
            )))
        })?;
        self.auth_token = Some(auth_token);
        Ok(())
    }

    /// Fetch the terms-of-service state and store the DFE cookie.
    ///
    /// # Errors
    ///
    /// Returns an error if the `toc` request fails, if the payload is
    /// invalid, if updated terms must be accepted, or if no DFE cookie is
    /// returned.
    pub async fn toc(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let resp = self
            .execute_request("toc", None, None, self.get_default_headers()?)
            .await?;
        let toc_response = resp
            .payload
            .ok_or_else(|| Box::new(GpapiError::from("Invalid payload.")))?
            .toc_response
            .ok_or_else(|| Box::new(GpapiError::from("Invalid toc response.")))?;
        let tos_present =
            toc_response.tos_token.is_some() || toc_response.tos_content.is_some();
        if tos_present {
            // Fresh sessions present ToS alongside the session cookie: accept
            // inline like the reference clients, then fall through — the
            // cookie in this same response is already valid.
            self.tos_token.clone_from(&toc_response.tos_token);
            let acknowledged = self.accept_tos().await?.is_some();
            if !acknowledged {
                return Err(Box::new(GpapiError::from(
                    "Play Store acceptTos was not acknowledged by the server",
                )));
            }
        }
        if let Some(cookie) = toc_response.cookie {
            self.dfe_cookie = Some(cookie);
            Ok(())
        } else if tos_present {
            Err(Box::new(GpapiError::new(GpapiErrorKind::TermsOfService)))
        } else {
            Err("No DFE cookie found.".into())
        }
    }

    /// Accept the play store terms of service.
    ///
    /// # Errors
    ///
    /// Returns an error if no `ToS` token was stored by a prior `toc` call or
    /// if the `acceptTos` request fails.
    pub async fn accept_tos(
        &mut self,
    ) -> Result<Option<AcceptTosResponse>, Box<dyn Error + Send + Sync>> {
        if let Some(tos_token) = &self.tos_token {
            let form_body = {
                let mut params = HashMap::new();
                params.insert(String::from("tost"), tos_token.clone());
                params.insert(String::from("toscme"), String::from("false"));
                form_post(&params)
            };

            let mut headers = self.get_default_headers()?;
            headers.insert(
                "content-type",
                String::from("application/x-www-form-urlencoded"),
            );

            let resp = self
                .execute_request(
                    "acceptTos",
                    None,
                    Some(&form_body.into_bytes()),
                    headers,
                )
                .await?;
            Ok(resp.payload.and_then(|payload| payload.accept_tos_response))
        } else {
            Err("ToS token must be set by `toc` call first.".into())
        }
    }

    /// Lower level Play Store request, used by APIs but exposed for specialized
    /// requests. Returns a `ResponseWrapper` which depending on the request
    /// populates different fields/values.
    async fn execute_request(
        &self,
        endpoint: &str,
        query: Option<HashMap<&str, String>>,
        msg: Option<&[u8]>,
        headers: HashMap<&str, String>,
    ) -> Result<ResponseWrapper, Box<dyn Error + Send + Sync>> {
        let bytes = self
            .execute_request_helper(endpoint, query, msg, headers, true)
            .await?;
        let resp = ResponseWrapper::decode(&mut Cursor::new(bytes))?;
        Ok(resp)
    }

    //async fn execute_request_helper_hyper(
    //    &self,
    //    endpoint: &str,
    //    query: Option<HashMap<&str, String>>,
    //    msg: Option<&[u8]>,
    //    headers: HashMap<&str, String>,
    //    fdfe: bool,
    //) -> Result<Bytes, Box<dyn Error>> {
    //    let query = if let Some(query) = query {
    //        format!("?{}", query
    //            .iter()
    //            .map(|(k, v)| format!("{}={}", k, v))
    //            .collect::<Vec<String>>()
    //            .join("&")
    //        )
    //    } else {
    //        String::from("")
    //    };

    //    let url = if fdfe {
    //        format!("{}/fdfe/{}{}", consts::defaults::DEFAULT_BASE_URL, endpoint, query)
    //    } else {
    //        format!("{}/{}{}", consts::defaults::DEFAULT_BASE_URL, endpoint, query)
    //    };

    //    let mut req = if let Some(msg) = msg {
    //        Request::builder()
    //            .method(Method::POST)
    //            .uri(url)
    //            .body(Body::from(msg.to_owned()))
    //            .unwrap()
    //    } else {
    //        Request::builder()
    //            .method(Method::GET)
    //            .uri(url)
    //            .body(Body::empty())
    //            .unwrap()
    //    };
    //    let hyper_headers = req.headers_mut();

    //    for (key, val) in headers {
    //        hyper_headers.insert(HyperHeaderName::from_bytes(key.as_bytes())?, HyperHeaderValue::from_str(&val)?);
    //    }

    //    let res = self.hyper_client.request(req).await?;

    //    let body_bytes = hyper::body::to_bytes(res.into_body()).await?;
    //    Ok(body_bytes)
    //}

    async fn execute_request_helper(
        &self,
        endpoint: &str,
        query: Option<HashMap<&str, String>>,
        msg: Option<&[u8]>,
        headers: HashMap<&str, String>,
        fdfe: bool,
    ) -> Result<Bytes, Box<dyn Error + Send + Sync>> {
        let mut url = if fdfe {
            Url::parse(&format!(
                "{}/fdfe/{}",
                consts::defaults::DEFAULT_BASE_URL,
                endpoint
            ))?
        } else {
            Url::parse(&format!(
                "{}/{}",
                consts::defaults::DEFAULT_BASE_URL,
                endpoint
            ))?
        };

        if let Some(query) = query {
            let mut queries = url.query_pairs_mut();
            for (key, val) in query {
                queries.append_pair(key, &val);
            }
        }

        let mut reqwest_headers = HeaderMap::new();
        for (key, val) in headers {
            reqwest_headers.insert(
                HeaderName::from_bytes(key.as_bytes())?,
                HeaderValue::from_str(&val)?,
            );
        }

        let res = {
            if let Some(msg) = msg {
                (*self.client)
                    .post(url)
                    .headers(reqwest_headers)
                    .body(msg.to_owned())
                    .send()
                    .await?
            } else {
                (*self.client)
                    .get(url)
                    .headers(reqwest_headers)
                    .send()
                    .await?
            }
        };

        Ok(res.bytes().await?)
    }
}

fn parse_form_reply(data: &str) -> HashMap<String, String> {
    let mut form_resp = HashMap::new();
    for line in data.split_terminator('\n') {
        if let Some((key, value)) = line.split_once('=') {
            form_resp.insert(key.to_lowercase(), value.to_string());
        } else {
            form_resp.insert(line.to_lowercase(), String::new());
        }
    }
    form_resp
}

#[derive(Debug, Clone)]
struct AuthRequest {
    params: HashMap<String, String>,
}

impl AuthRequest {
    fn new(email: &str, oauth_token: &str) -> Self {
        let mut auth_request = Self::default();
        auth_request
            .params
            .insert(String::from("Email"), String::from(email));
        auth_request
            .params
            .insert(String::from("Token"), String::from(oauth_token));
        auth_request
    }
}

impl Default for AuthRequest {
    fn default() -> Self {
        let mut params = HashMap::new();
        params.insert(
            String::from("lang"),
            String::from(consts::defaults::DEFAULT_LANGUAGE),
        );
        params.insert(
            String::from("google_play_services_version"),
            String::from(consts::defaults::DEFAULT_GOOGLE_PLAY_SERVICES_VERSION),
        );
        params.insert(
            String::from("sdk_version"),
            String::from(consts::defaults::api_user_agent::DEFAULT_SDK),
        );
        params.insert(
            String::from("device_country"),
            String::from(consts::defaults::DEFAULT_COUNTRY_CODE),
        );
        params.insert(String::from("Email"), String::new());
        params.insert(
            String::from("service"),
            String::from(consts::defaults::DEFAULT_SERVICE),
        );
        params.insert(String::from("get_accountid"), String::from("1"));
        params.insert(String::from("ACCESS_TOKEN"), String::from("1"));
        params.insert(
            String::from("callerPkg"),
            String::from(consts::defaults::DEFAULT_ANDROID_VENDING),
        );
        params.insert(String::from("add_account"), String::from("1"));
        params.insert(String::from("Token"), String::new());
        params.insert(
            String::from("callerSig"),
            String::from(consts::defaults::DEFAULT_CALLER_SIG),
        );
        Self { params }
    }
}

fn form_post(params: &HashMap<String, String>) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<String>>()
        .join("&")
}

#[derive(Debug, Clone)]
struct BuildConfiguration {
    pub finsky_agent: String,
    pub finsky_version: String,
    pub api: String,
    pub version_code: String,
    pub sdk: String,
    pub device: String,
    pub hardware: String,
    pub product: String,
    pub platform_version_release: String,
    pub model: String,
    pub build_id: String,
    pub is_wide_screen: String,
    pub supported_abis: String,
}

impl BuildConfiguration {
    #[must_use]
    pub fn user_agent(&self) -> String {
        let finsky_agent = &self.finsky_agent;
        let finsky_version = &self.finsky_version;
        let api = &self.api;
        let version_code = &self.version_code;
        let sdk = &self.sdk;
        let device = &self.device;
        let hardware = &self.hardware;
        let product = &self.product;
        let platform_version_release = &self.platform_version_release;
        let model = &self.model;
        let build_id = &self.build_id;
        let is_wide_screen = &self.is_wide_screen;
        let supported_abis = &self.supported_abis;
        format!("{finsky_agent}/{finsky_version} (api={api},versionCode={version_code},sdk={sdk},device={device},hardware={hardware},product={product},platformVersionRelease={platform_version_release},model={model},buildId={build_id},isWideScreen={is_wide_screen},supportedAbis={supported_abis})")
    }
}

impl BuildConfiguration {
    #[allow(clippy::too_many_arguments)]
    fn new(
        finsky_version: &str,
        version_code: &str,
        sdk: &str,
        device: &str,
        hardware: &str,
        product: &str,
        platform_version_release: &str,
        model: &str,
        build_id: &str,
        supported_abis: &str,
    ) -> Self {
        use consts::defaults::api_user_agent::{DEFAULT_API, DEFAULT_IS_WIDE_SCREEN};
        use consts::defaults::DEFAULT_FINSKY_AGENT;

        Self {
            finsky_agent: DEFAULT_FINSKY_AGENT.to_string(),
            finsky_version: finsky_version.to_string(),
            api: DEFAULT_API.to_string(),
            version_code: version_code.to_string(),
            sdk: sdk.to_string(),
            device: device.to_string(),
            hardware: hardware.to_string(),
            product: product.to_string(),
            platform_version_release: platform_version_release.to_string(),
            model: model.to_string(),
            build_id: build_id.to_string(),
            is_wide_screen: DEFAULT_IS_WIDE_SCREEN.to_string(),
            supported_abis: supported_abis.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_form() {
        let form_reply = "FOO=BAR\nbaz=qux";
        let mut expected_reply = HashMap::new();
        expected_reply.insert("baz".to_string(), "qux".to_string());
        expected_reply.insert("foo".to_string(), "BAR".to_string());
        let parsed_form_reply = parse_form_reply(form_reply);
        assert_eq!(expected_reply, parsed_form_reply);
    }

    mod gpapi {
        use googleplay_protobuf::BulkDetailsRequest;

        #[test]
        fn test_protobuf() {
            let _bdr = BulkDetailsRequest {
                doc_id: vec!["test".to_string()],
                include_child_docs: Some(true),
                ..Default::default()
            };
        }
    }
}
