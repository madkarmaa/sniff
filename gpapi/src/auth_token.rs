use crate::Gpapi;
use crate::auth::parse_auth_reply;
use crate::consts;
use crate::error::{Error as GpapiError, ErrorKind as GpapiErrorKind};
use crate::headers::invalid_response;
use std::collections::HashMap;
use std::error::Error;

impl Gpapi {
    pub(super) async fn request_auth_token(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
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
            self.append_auth_headers(&mut headers)?;
            headers.insert("content-length", String::from("0"));
            headers
        };

        // Not `execute_request_helper`: `auth` explains failures in 4xx bodies.
        let (status, bytes) = self
            .send_request("auth", Some(form_params), Some(&[]), headers, false)
            .await?;

        let reply = parse_auth_reply(status, &bytes)?;
        let auth_token = reply.get("auth").cloned().ok_or_else(|| {
            let google_error = reply.get("error").cloned().unwrap_or_default();
            Box::new(GpapiError::from(format!(
                "authentication failed: {google_error}"
            )))
        })?;
        self.auth_token = Some(auth_token);
        Ok(())
    }

    fn append_default_auth_params(
        &self,
        params: &mut HashMap<&str, String>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Some(gsf_id) = self.gsf_id {
            params.insert("androidId", format!("{gsf_id:x}"));
        }

        let build = self.build()?;
        params.insert(
            "sdk_version",
            build.sdk_version.ok_or_else(invalid_response)?.to_string(),
        );
        params.insert("Email", self.email.clone());
        params.insert(
            "google_play_services_version",
            build
                .google_services
                .ok_or_else(invalid_response)?
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
}
