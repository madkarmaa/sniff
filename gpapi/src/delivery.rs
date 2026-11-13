use crate::Gpapi;
use crate::error::{Error as GpapiError, ErrorKind as GpapiErrorKind};
use std::collections::HashMap;
use std::error::Error;

/// A split APK or an additional file, with the size and hashes Google reports for it.
///
/// `sha1` and `sha256` are passed through exactly as Google sends them
/// (typically base64url-style strings, not hex); they are not transformed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NamedDownload {
    pub name: Option<String>,
    pub download_url: Option<String>,
    /// Download size in bytes.
    pub size: Option<i64>,
    pub sha1: Option<String>,
    pub sha256: Option<String>,
}

/// Download URLs returned by the Play Store delivery endpoint.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadInfo {
    pub main_apk_url: Option<String>,
    /// Main APK download size in bytes.
    pub main_apk_size: Option<i64>,
    /// Main APK SHA-1 as sent by Google (typically base64url-style, not hex).
    pub main_apk_sha1: Option<String>,
    /// Main APK SHA-256 as sent by Google (typically base64url-style, not hex).
    pub main_apk_sha256: Option<String>,
    pub splits: Vec<NamedDownload>,
    /// Expansion files, named `main.<version>.<package>.obb` or `patch.<version>.<package>.obb`.
    pub additional_files: Vec<NamedDownload>,
    pub dex_metadata_url: Option<String>,
}

impl Gpapi {
    /// Retrieve the download URL(s) and names for a package, given a package ID and optional
    /// version code.
    ///
    /// # Arguments
    ///
    /// * `pkg_name` - A string type specifying the package's app ID, e.g. `com.instagram.android`
    /// * `version_code` - An optional version code. If omitted, the latest version is used.
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
            let mut params = HashMap::new();
            params.insert("ot", String::from("1"));
            params.insert("doc", String::from(&pkg_name));
            params.insert("vc", version_code.to_string());

            let mut headers = self.get_default_headers()?;
            headers.insert("content-length", String::from("0"));

            self.execute_request("purchase", Some(params), Some(&[]), headers)
                .await?
        };
        if let Some(payload) = resp.payload
            && let Some(buy_response) = payload.buy_response
            && let Some(delivery_token) = buy_response.encoded_delivery_token
        {
            return self.delivery(&pkg_name, version_code, delivery_token).await;
        }
        Err(Box::new(GpapiError::new(GpapiErrorKind::InvalidApp)))
    }

    async fn delivery(
        &self,
        pkg_name: &str,
        version_code: i64,
        delivery_token: String,
    ) -> Result<DownloadInfo, Box<dyn Error + Send + Sync>> {
        let resp = {
            let mut req = HashMap::new();
            req.insert("ot", String::from("1"));
            req.insert("doc", String::from(pkg_name));
            req.insert("vc", version_code.to_string());
            req.insert("dtok", delivery_token);
            self.execute_request("delivery", Some(req), None, self.get_default_headers()?)
                .await?
        };
        if let Some(payload) = resp.payload
            && let Some(delivery_response) = payload.delivery_response
            && let Some(app_delivery_data) = delivery_response.app_delivery_data
        {
            let splits = app_delivery_data
                .split_delivery_data
                .into_iter()
                .map(|split| NamedDownload {
                    name: split.name,
                    download_url: split.download_url,
                    size: split.download_size,
                    sha1: split.sha1,
                    sha256: split.sha256,
                })
                .collect();
            let additional_files = app_delivery_data
                .additional_file
                .into_iter()
                .filter_map(|file| {
                    let kind = if file.file_type? == 0 {
                        "main"
                    } else {
                        "patch"
                    };
                    let filename = format!("{kind}.{}.{pkg_name}.obb", file.version_code?);
                    // Expansion files carry no sha256 in the delivery protobuf.
                    Some(NamedDownload {
                        name: Some(filename),
                        download_url: file.download_url,
                        size: file.size,
                        sha1: file.sha1,
                        sha256: None,
                    })
                })
                .collect();
            return Ok(DownloadInfo {
                main_apk_url: app_delivery_data.download_url,
                main_apk_size: app_delivery_data.download_size,
                main_apk_sha1: app_delivery_data.sha1,
                main_apk_sha256: app_delivery_data.sha256,
                splits,
                additional_files,
                dex_metadata_url: app_delivery_data
                    .dex_metadata
                    .and_then(|dex_metadata| dex_metadata.download_url),
            });
        }
        Err(Box::new(GpapiError::new(GpapiErrorKind::InvalidApp)))
    }
}

#[cfg(test)]
#[path = "tests/delivery.rs"]
mod tests;
