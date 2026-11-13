use crate::Gpapi;
use crate::consts;
use crate::error::{Error as GpapiError, ErrorKind as GpapiErrorKind};
use bytes::Bytes;
use googleplay_protobuf::ResponseWrapper;
use prost::Message;
use reqwest::Url;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::HashMap;
use std::error::Error;
use std::io::Cursor;

impl Gpapi {
    /// Lower level Play Store request, used by APIs but exposed for specialized
    /// requests. Returns a `ResponseWrapper` which depending on the request
    /// populates different fields/values.
    pub(super) async fn execute_request(
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

    /// Like `send_request`, but turns non-success statuses into an HTTP error
    /// instead of handing the body to a decoder that would misreport it.
    pub(super) async fn execute_request_helper(
        &self,
        endpoint: &str,
        query: Option<HashMap<&str, String>>,
        msg: Option<&[u8]>,
        headers: HashMap<&str, String>,
        fdfe: bool,
    ) -> Result<Bytes, Box<dyn Error + Send + Sync>> {
        let (status, body) = self
            .send_request(endpoint, query, msg, headers, fdfe)
            .await?;
        if is_success_status(status) {
            Ok(body)
        } else {
            Err(http_error(status, &body))
        }
    }

    /// Sends the request and returns the status with the raw body, without
    /// judging the status.
    pub(super) async fn send_request(
        &self,
        endpoint: &str,
        query: Option<HashMap<&str, String>>,
        msg: Option<&[u8]>,
        headers: HashMap<&str, String>,
        fdfe: bool,
    ) -> Result<(u16, Bytes), Box<dyn Error + Send + Sync>> {
        let base_url = consts::defaults::DEFAULT_BASE_URL;
        let mut url = if fdfe {
            Url::parse(&format!("{base_url}/fdfe/{endpoint}"))?
        } else {
            Url::parse(&format!("{base_url}/{endpoint}"))?
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
                self.client
                    .post(url)
                    .headers(reqwest_headers)
                    .body(msg.to_owned())
                    .send()
                    .await?
            } else {
                self.client.get(url).headers(reqwest_headers).send().await?
            }
        };

        let status = res.status().as_u16();
        Ok((status, res.bytes().await?))
    }
}

pub const fn is_success_status(status: u16) -> bool {
    matches!(status, 200..=299)
}

/// Longest response excerpt kept in an HTTP error.
const MAX_ERROR_BODY_CHARS: usize = 200;

pub fn http_error(status: u16, body: &[u8]) -> Box<dyn Error + Send + Sync> {
    // Truncate by chars, not bytes, so a multi-byte sequence is never split.
    let body = String::from_utf8_lossy(body)
        .trim()
        .chars()
        .take(MAX_ERROR_BODY_CHARS)
        .collect();
    Box::new(GpapiError::new(GpapiErrorKind::Http { status, body }))
}

#[cfg(test)]
#[path = "tests/transport.rs"]
mod tests;
