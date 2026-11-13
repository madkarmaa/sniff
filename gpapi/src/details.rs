use crate::Gpapi;
use crate::error::{Error as GpapiError, ErrorKind as GpapiErrorKind};
use googleplay_protobuf::{BulkDetailsRequest, BulkDetailsResponse, DetailsResponse};
use prost::Message;
use std::collections::HashMap;
use std::error::Error;

impl Gpapi {
    pub(super) async fn get_latest_version_for_pkg_name(
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
        let req = BulkDetailsRequest {
            doc_id: pkg_names.iter().copied().map(String::from).collect(),
            include_child_docs: Some(false),
            ..Default::default()
        };
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
}
