use crate::Gpapi;
use crate::auth::form_post;
use crate::error::{Error as GpapiError, ErrorKind as GpapiErrorKind};
use googleplay_protobuf::AcceptTosResponse;
use std::collections::HashMap;
use std::error::Error;

impl Gpapi {
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
            self.authenticate().await
        } else {
            Err("No device config token".into())
        }
    }

    /// Renew the session (auth token and DFE cookie) of the registered device,
    /// without a check-in that would register another one. Without a
    /// registered device this is a full [`Gpapi::login`].
    ///
    /// # Errors
    ///
    /// Returns the errors of [`Gpapi::login`].
    pub async fn reauthenticate(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        if self.has_registered_device() {
            self.authenticate().await
        } else {
            self.login().await
        }
    }

    async fn authenticate(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // A stale session must not leak into the requests that renew it.
        self.auth_token = None;
        self.dfe_cookie = None;
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
        let tos_present = toc_response.tos_token.is_some() || toc_response.tos_content.is_some();
        if tos_present {
            // Fresh sessions present the ToS along with the session cookie.
            // Accept them inline like the reference clients and carry on, since
            // the cookie in this same response is already valid.
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
                .execute_request("acceptTos", None, Some(&form_body.into_bytes()), headers)
                .await?;
            Ok(resp.payload.and_then(|payload| payload.accept_tos_response))
        } else {
            Err("ToS token must be set by `toc` call first.".into())
        }
    }
}
