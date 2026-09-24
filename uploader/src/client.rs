// Native errors and Vec/Bytes differ by target; JS futures are single-threaded.
#![cfg_attr(
    target_arch = "wasm32",
    allow(
        clippy::future_not_send,
        clippy::ignored_unit_patterns,
        clippy::implicit_clone
    )
)]
//! Google Photos native protocol. Credentials, bodies and opaque URLs never enter errors.
use crate::cred::{Credential, auth_form_body};
use crate::proto::{
    self, ANDROID_API_VERSION, BUILD_FINGERPRINT, CLIENT_VERSION_CODE, PIXEL_MAKE, PIXEL_MODEL,
    QUALITY_ORIGINAL,
};
use crate::transport::{Response, send};
use sha1::Digest as _;
use std::fmt;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(target_arch = "wasm32")]
use web_time::{SystemTime, UNIX_EPOCH};

const AUTH_URL: &str = "https://android.googleapis.com/auth";
const UPLOAD_URL: &str = "https://photos.googleapis.com/data/upload/uploadmedia/interactive";
const HASH_URL: &str =
    "https://photosdata-pa.googleapis.com/6439526531001121323/5084965799730810217";
const COMMIT_URL: &str =
    "https://photosdata-pa.googleapis.com/6439526531001121323/16538846908252377752";
const DOWNLOAD_URL: &str = "https://photosdata-pa.googleapis.com/$rpc/social.frontend.photos.preparedownloaddata.v1.PhotosPrepareDownloadDataService/PhotosPrepareDownload";

/// Sanitized operation failure; never retry an uncertain commit without checking Photos.
#[derive(Debug)]
pub struct Error {
    pub stage: &'static str,
    pub completion_uncertain: bool,
    pub http_status: Option<u16>,
    pub native_status: Option<u32>,
}
impl Error {
    const fn at(stage: &'static str) -> Self {
        Self {
            stage,
            completion_uncertain: false,
            http_status: None,
            native_status: None,
        }
    }
    const fn uncertain(mut self) -> Self {
        self.completion_uncertain = true;
        self
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} failed (HTTP {:?}, native {:?}, completion uncertain: {})",
            self.stage, self.http_status, self.native_status, self.completion_uncertain
        )
    }
}
impl std::error::Error for Error {}

/// One HTTP session, its credential, and a refreshed-on-demand access token.
pub struct PhotosClient {
    cred: Credential,
    http: reqwest::Client,
    bearer: Option<(String, i64)>,
}
impl PhotosClient {
    /// Create a client with redirects and automatic retries disabled.
    /// # Errors
    /// Returns a sanitized error if the HTTP client cannot be built.
    pub fn new(cred: Credential) -> Result<Self, Error> {
        let builder = reqwest::Client::builder();
        #[cfg(not(target_arch = "wasm32"))]
        let builder = builder
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(120));
        let http = builder.build().map_err(|_| Error::at("client"))?;
        Ok(Self {
            cred,
            http,
            bearer: None,
        })
    }
    #[must_use]
    pub fn user_agent(&self) -> String {
        format!(
            "com.google.android.apps.photos/{CLIENT_VERSION_CODE} (Linux; U; Android 9; {}; {PIXEL_MODEL}; Build/{BUILD_FINGERPRINT}; Cronet/127.0.6510.5) (gzip)",
            self.cred.lang
        )
    }
    fn now_unix() -> Result<i64, Error> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::at("clock"))?;
        i64::try_from(now.as_secs()).map_err(|_| Error::at("clock"))
    }
    /// Exchange the account credential, caching the bearer until shortly before expiry.
    /// # Errors
    /// Rejects failed/redirected requests, server rejection and malformed responses.
    pub async fn bearer_token(&mut self) -> Result<String, Error> {
        let now = Self::now_unix()?;
        if let Some((token, expiry)) = &self.bearer
            && now < expiry.saturating_sub(60)
        {
            return Ok(token.clone());
        }
        let form = auth_form_body(&self.cred).map_err(|_| Error::at("credentials"))?;
        let response = send(
            self.http
                .post(AUTH_URL)
                .header("accept-encoding", "identity")
                .header("app", crate::cred::PHOTOS_APP)
                .header("content-type", "application/x-www-form-urlencoded")
                .header("device", &self.cred.android_id)
                .header(
                    "user-agent",
                    "GoogleAuth/1.4 (Pixel XL PQ2A.190205.001); gzip",
                )
                .body(form),
        )
        .await
        .map_err(|_| Error::at("authentication"))?;
        let body = response_bytes(response, "authentication").await?;
        let text = std::str::from_utf8(&body).map_err(|_| Error::at("authentication response"))?;
        let parsed = proto::parse_auth_response(text);
        if parsed.contains_key("Error") {
            return Err(Error::at("authentication rejected"));
        }
        let token = parsed
            .get("Auth")
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::at("authentication response"))?;
        let expiry = parsed
            .get("Expiry")
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|&t| t > now)
            .ok_or_else(|| Error::at("authentication expiry"))?;
        self.bearer = Some((token.clone(), expiry));
        Ok(token.clone())
    }
    async fn request(
        &mut self,
        method: reqwest::Method,
        url: url::Url,
    ) -> Result<reqwest::RequestBuilder, Error> {
        let bearer = self.bearer_token().await?;
        Ok(self
            .http
            .request(method, url)
            .bearer_auth(bearer)
            .header("user-agent", self.user_agent())
            .header("accept-encoding", "identity")
            .header("accept-language", &self.cred.lang))
    }
    async fn rpc(
        &mut self,
        endpoint: &str,
        body: Vec<u8>,
        stage: &'static str,
    ) -> Result<Vec<u8>, Error> {
        let url = url::Url::parse(endpoint).map_err(|_| Error::at(stage))?;
        let request = self
            .request(reqwest::Method::POST, url)
            .await?
            .header("content-type", "application/x-protobuf")
            .header("x-goog-ext-173412678-bin", "CgcIAhClARgC")
            .header("x-goog-ext-174067345-bin", "CgIIAg==")
            .body(body);
        let submitted_error = |mut error: Error| {
            if stage == "commit" {
                error.completion_uncertain = true;
            }
            error
        };
        let response = send(request)
            .await
            .map_err(|_| submitted_error(Error::at(stage)))?;
        response_bytes(response, stage)
            .await
            .map_err(submitted_error)
    }
    /// Look up an exact file fingerprint before upload.
    /// # Errors
    /// Fails closed on malformed/mismatched responses; never interprets them as absence.
    pub async fn find_remote_media_by_hash(
        &mut self,
        sha1: &[u8; 20],
    ) -> Result<Option<String>, Error> {
        let request = proto::encode_hash_check(sha1).map_err(|_| Error::at("hash input"))?;
        let response = self.rpc(HASH_URL, request, "hash lookup").await?;
        proto::remote_media_key(&response, sha1).map_err(|_| Error::at("hash response"))
    }
    /// Start a transfer session; this does not create a library item.
    /// # Errors
    /// Rejects failed responses or a missing upload identifier.
    pub async fn get_upload_token(
        &mut self,
        sha1_b64: &str,
        file_size: u64,
    ) -> Result<String, Error> {
        if file_size == 0 {
            return Err(Error::at("empty upload"));
        }
        let body =
            proto::encode_get_upload_token(file_size).map_err(|_| Error::at("upload input"))?;
        let url = url::Url::parse(UPLOAD_URL).map_err(|_| Error::at("upload URL"))?;
        let response = send(
            self.request(reqwest::Method::POST, url)
                .await?
                .header("content-type", "application/x-protobuf")
                .header("x-goog-hash", format!("sha1={sha1_b64}"))
                .header("x-upload-content-length", file_size)
                .body(body),
        )
        .await
        .map_err(|_| Error::at("upload start"))?;
        check_status(&response, "upload start")?;
        response
            .headers()
            .get("X-GUploader-UploadID")
            .and_then(|v| v.to_str().ok())
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| Error::at("upload start response"))
    }
    /// Transfer bytes once, returning the opaque finalize token in memory.
    /// # Errors
    /// Returns a sanitized transfer error without exposing the upload URL.
    pub async fn put_upload(&mut self, upload_id: &str, bytes: Vec<u8>) -> Result<Vec<u8>, Error> {
        let mut url = url::Url::parse(UPLOAD_URL).map_err(|_| Error::at("upload URL"))?;
        url.query_pairs_mut().append_pair("upload_id", upload_id);
        let response = send(self.request(reqwest::Method::PUT, url).await?.body(bytes))
            .await
            .map_err(|_| Error::at("upload PUT"))?;
        let bytes = response_bytes(response, "upload PUT").await?;
        proto::scotty_opaque(&bytes).map_err(|_| Error::at("transfer token"))?;
        Ok(bytes)
    }
    /// Create the library item; no retries, including transport-level retries.
    /// # Errors
    /// `completion_uncertain` is set for unrecognized replies or errors after submission.
    pub async fn commit_upload(
        &mut self,
        scotty: &[u8],
        file_name: &str,
        sha1: &[u8; 20],
        modified_unix: i64,
    ) -> Result<String, Error> {
        if file_name.is_empty() || file_name.contains(['\\', '/', '\r', '\n']) {
            return Err(Error::at("filename"));
        }
        let (f1, f2) = proto::decode_commit_token(scotty).map_err(|_| Error::at("commit input"))?;
        let body = proto::encode_commit_upload(
            f1,
            &f2,
            file_name,
            sha1,
            modified_unix,
            PIXEL_MODEL,
            PIXEL_MAKE,
            ANDROID_API_VERSION,
            QUALITY_ORIGINAL,
        )
        .map_err(|_| Error::at("commit input"))?;
        let response = self.rpc(COMMIT_URL, body, "commit").await?;
        let media = proto::commit_media_key(&response, scotty)
            .map_err(|_| Error::at("commit response").uncertain())?;
        media.ok_or(Error {
            stage: "commit rejected",
            completion_uncertain: false,
            http_status: None,
            native_status: Some(10),
        })
    }
    /// Resolve a media key through the native API and verify the downloaded original's SHA-1.
    /// # Errors
    /// Rejects unexpected hosts, redirects, incomplete metadata and changed bytes.
    pub async fn download(&mut self, media_id: &str) -> Result<Vec<u8>, Error> {
        let body = proto::prepare_download(media_id).map_err(|_| Error::at("download input"))?;
        let response = self.rpc(DOWNLOAD_URL, body, "prepare download").await?;
        let (url, sha1) = proto::original_download(&response, media_id)
            .map_err(|_| Error::at("download metadata"))?;
        let bytes = self.download_url(&url).await?;
        if sha1::Sha1::digest(&bytes).as_slice() != sha1 {
            return Err(Error::at("download integrity"));
        }
        Ok(bytes)
    }
    /// Fetch an exact original URL from the observed content host, with no redirects.
    /// # Errors
    /// Rejects untrusted URLs and non-image responses. Prefer `download` for hash verification.
    pub async fn download_url(&mut self, url: &str) -> Result<Vec<u8>, Error> {
        let url = original_url(url)?;
        let response = send(self.request(reqwest::Method::GET, url).await?)
            .await
            .map_err(|_| Error::at("download GET"))?;
        check_status(&response, "download GET")?;
        if response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_none_or(|v| !v.starts_with("image/"))
        {
            return Err(Error::at("download content type"));
        }
        response
            .bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|_| Error::at("download bytes"))
    }
}
fn original_url(raw: &str) -> Result<url::Url, Error> {
    let url = url::Url::parse(raw).map_err(|_| Error::at("download URL"))?;
    if url.scheme() != "https"
        || url.host_str() != Some("lh3.googleusercontent.com")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::at("download URL"));
    }
    Ok(url)
}
fn check_status(response: &Response, stage: &'static str) -> Result<(), Error> {
    if response.status() != reqwest::StatusCode::OK {
        return Err(Error {
            http_status: Some(response.status().as_u16()),
            ..Error::at(stage)
        });
    }
    Ok(())
}
async fn response_bytes(mut response: Response, stage: &'static str) -> Result<Vec<u8>, Error> {
    check_status(&response, stage)?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Error::at(stage))? {
        if bytes.len().saturating_add(chunk.len()) > 2 * 1024 * 1024 {
            return Err(Error::at("oversized control response"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bearer_destinations_and_errors_are_restricted() {
        for raw in [
            "http://lh3.googleusercontent.com/a",
            "https://lh3.googleusercontent.com.evil.test/a",
            "https://lh3.googleusercontent.com:444/a",
            "https://user@lh3.googleusercontent.com/a",
            "https://accounts.google.com/a",
            "https://lh3.googleusercontent.com/a#fragment",
        ] {
            assert!(original_url(raw).is_err());
        }
        let raw = "https://lh3.googleusercontent.com/original";
        assert_eq!(original_url(raw).unwrap().as_str(), raw);
        assert!(Error::at("commit").uncertain().completion_uncertain);
        assert!(!Error::at("input").completion_uncertain);
    }
}
