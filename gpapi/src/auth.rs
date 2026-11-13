use crate::Gpapi;
use crate::consts;
use crate::error::Error as GpapiError;
use crate::transport::{http_error, is_success_status};
use std::collections::HashMap;
use std::error::Error;

impl Gpapi {
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
        let resp = self.request_aas_token_helper(&auth_req).await?;
        self.aas_token = Some(aas_token_from_reply(resp)?);
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

        // Not `execute_request_helper`: `auth` explains failures in 4xx bodies.
        let (status, body_bytes) = self
            .send_request("auth", None, Some(&form_body.into_bytes()), headers, false)
            .await?;

        parse_auth_reply(status, &body_bytes)
    }
}

/// Parses a form-encoded reply from the `auth` endpoint. That endpoint reports
/// failures as `Error=...` bodies on 4xx statuses, so a non-success status is
/// only an HTTP error when the body carries no such explanation (e.g. a 5xx
/// or 429 page).
pub fn parse_auth_reply(
    status: u16,
    body: &[u8],
) -> Result<HashMap<String, String>, Box<dyn Error + Send + Sync>> {
    match std::str::from_utf8(body) {
        Ok(text) => {
            let reply = parse_form_reply(text);
            if is_success_status(status) || reply.contains_key("error") {
                Ok(reply)
            } else {
                Err(http_error(status, body))
            }
        }
        Err(e) if is_success_status(status) => Err(Box::new(e)),
        Err(_) => Err(http_error(status, body)),
    }
}

/// Extracts the aas token from a parsed `auth` reply. A reply without one is
/// reported with Google's own reason (e.g. `MissingDroidguard`); the message
/// never contains the token or the account email.
fn aas_token_from_reply(
    mut reply: HashMap<String, String>,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    reply.remove("token").ok_or_else(|| {
        let google_error = reply
            .get("error")
            .filter(|error| !error.is_empty())
            .map_or("no token in reply", String::as_str);
        Box::<dyn Error + Send + Sync>::from(GpapiError::from(format!(
            "authentication failed: {google_error}"
        )))
    })
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
        let params = [
            ("lang", consts::defaults::DEFAULT_LANGUAGE),
            (
                "google_play_services_version",
                consts::defaults::DEFAULT_GOOGLE_PLAY_SERVICES_VERSION,
            ),
            ("sdk_version", consts::defaults::api_user_agent::DEFAULT_SDK),
            ("device_country", consts::defaults::DEFAULT_COUNTRY_CODE),
            ("Email", email),
            ("service", consts::defaults::DEFAULT_SERVICE),
            ("get_accountid", "1"),
            ("ACCESS_TOKEN", "1"),
            ("callerPkg", consts::defaults::DEFAULT_ANDROID_VENDING),
            ("add_account", "1"),
            ("Token", oauth_token),
            ("callerSig", consts::defaults::DEFAULT_CALLER_SIG),
            // Google rejects the exchange with `MissingDroidguard` without it; the value is not validated.
            (
                "droidguard_results",
                consts::defaults::DEFAULT_DROIDGUARD_RESULTS,
            ),
        ]
        .into_iter()
        .map(|(k, v)| (String::from(k), String::from(v)))
        .collect();
        Self { params }
    }
}

pub fn form_post(params: &HashMap<String, String>) -> String {
    form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params)
        .finish()
}

#[cfg(test)]
#[path = "tests/auth.rs"]
mod tests;
