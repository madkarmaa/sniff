use crate::Gpapi;
use crate::error::{Error as GpapiError, ErrorKind as GpapiErrorKind};
use googleplay_protobuf::{
    BulkDetailsRequest, BulkDetailsResponse, DetailsResponse, ResponseWrapper,
};
use prost::Message;
use std::collections::HashMap;
use std::error::Error;

/// App details with what the public protobuf leaves out of them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AppListing {
    pub details: DetailsResponse,
    /// Names of the fast-follow asset packs of the listed version.
    pub fast_follow_packs: Vec<String>,
}

/// The file type of a fast-follow asset pack in `AppDetails.file`.
const FILE_TYPE_ASSET_PACK: i32 = 4;

// The path from the response to `AppDetails.file`, which carries the pack
// name in field 7. googleplay-protobuf's `FileMetadata` lacks that field.
#[derive(Clone, PartialEq, Message)]
struct ListingWrapper {
    #[prost(message, optional, tag = "1")]
    payload: Option<ListingPayload>,
}

#[derive(Clone, PartialEq, Message)]
struct ListingPayload {
    #[prost(message, optional, tag = "2")]
    details_response: Option<ListingDetailsResponse>,
}

#[derive(Clone, PartialEq, Message)]
struct ListingDetailsResponse {
    #[prost(message, optional, tag = "4")]
    item: Option<ListingItem>,
}

#[derive(Clone, PartialEq, Message)]
struct ListingItem {
    #[prost(message, optional, tag = "13")]
    details: Option<ListingDocumentDetails>,
}

#[derive(Clone, PartialEq, Message)]
struct ListingDocumentDetails {
    #[prost(message, optional, tag = "1")]
    app_details: Option<ListingAppDetails>,
}

#[derive(Clone, PartialEq, Message)]
struct ListingAppDetails {
    #[prost(message, repeated, tag = "17")]
    files: Vec<ListingFile>,
}

#[derive(Clone, PartialEq, Message)]
struct ListingFile {
    #[prost(int32, optional, tag = "1")]
    file_type: Option<i32>,
    #[prost(string, optional, tag = "7")]
    asset_pack_name: Option<String>,
}

fn fast_follow_packs(bytes: &[u8]) -> Result<Vec<String>, prost::DecodeError> {
    let files = ListingWrapper::decode(bytes)?
        .payload
        .and_then(|payload| payload.details_response)
        .and_then(|response| response.item)
        .and_then(|item| item.details)
        .and_then(|details| details.app_details)
        .map_or_default(|app_details| app_details.files);
    Ok(files
        .into_iter()
        .filter(|file| file.file_type == Some(FILE_TYPE_ASSET_PACK))
        .filter_map(|file| file.asset_pack_name)
        .collect())
}

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
        Ok(self.listing(pkg_name).await?.map(|listing| listing.details))
    }

    /// Like `details`, and also reports the app's fast-follow asset packs.
    ///
    /// # Errors
    ///
    /// Returns an error if login is required or if the details request fails.
    pub async fn listing<S: Into<String>>(
        &self,
        pkg_name: S,
    ) -> Result<Option<AppListing>, Box<dyn Error + Send + Sync>> {
        if self.auth_token.is_none() {
            return Err(Box::new(GpapiError::new(GpapiErrorKind::LoginRequired)));
        }
        let mut form_params = HashMap::new();
        form_params.insert("doc", pkg_name.into());

        let headers = self.get_default_headers()?;

        let bytes = self
            .execute_request_helper("details", Some(form_params), None, headers, true)
            .await?;
        let details = ResponseWrapper::decode(&*bytes)?
            .payload
            .and_then(|payload| payload.details_response);
        let Some(details) = details else {
            return Ok(None);
        };
        Ok(Some(AppListing {
            details,
            fast_follow_packs: fast_follow_packs(&bytes)?,
        }))
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

#[cfg(test)]
#[path = "tests/details.rs"]
mod tests;
