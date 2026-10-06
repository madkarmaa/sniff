use crate::client_registry::SharedClientRegistry;
use crate::google_play_client::Channel;
use crate::openapi_schema::{
    ApiResponse, DownloadInfo, DownloadResponse, ErrorResponse, SerializableDetailsResponse,
};
use crate::serializable_types::SerializableDetailsResponse as JsonDetails;
use std::str::FromStr;
use worker::{Response, Result};

fn error_response(status: u16, message: String) -> Result<Response> {
    Ok(Response::from_json(&ApiResponse::<()> {
        success: false,
        data: None,
        error: Some(message),
    })?
    .with_status(status))
}

#[utoipa::path(
    get,
    path = "/v2/download/{package_name}/{channel}",
    params(
        ("package_name" = String, Path, description = "Android package name"),
        ("channel" = String, Path, description = "Release channel", example = "stable")
    ),
    responses(
        (status = 200, description = "Latest app details and download info retrieved successfully",
         body = ApiResponse<DownloadResponse<SerializableDetailsResponse>>,
         example = json!({
             "success": true,
             "data": {
                 "item": {
                     "id": "com.discord",
                     "title": "Discord - Talk, Play, Hang Out",
                     "creator": "Discord Inc.",
                     "details": {
                         "app_details": {
                             "version_code": 289_020,
                             "version_string": "289.20 - Stable",
                             "package_name": "com.discord"
                         }
                     }
                 },
                 "footer_html": "All prices include VAT.",
                 "enable_reviews": true,
                 "main_apk_url": "https://play.googleapis.com/download/by-token/download?token=AOTCm0Q...",
                 "splits": [
                     {
                         "name": "config.arm64_v8a",
                         "download_url": "https://play.googleapis.com/download/by-token/download?token=AOTCm0R..."
                     },
                     {
                         "name": "config.en",
                         "download_url": "https://play.googleapis.com/download/by-token/download?token=AOTCm0S..."
                     }
                 ],
                 "additional_files": [],
                 "dex_metadata_url": null
             },
             "error": null
         })
        ),
        (status = 400, description = "Invalid channel",
         body = ErrorResponse,
         example = json!({
             "success": false,
             "data": null,
             "error": "Invalid Channel: snapshot"
         })
        ),
        (status = 404, description = "App not found",
         body = ErrorResponse,
         example = json!({
             "success": false,
             "data": null,
             "error": "App 'com.discord' not found"
         })
        ),
        (status = 500, description = "Internal server error",
         body = ErrorResponse,
         example = json!({
             "success": false,
             "data": null,
             "error": "API error for stable channel: Invalid app response"
         })
        )
    ),
    tag = "Downloads"
)]
// Workers run on a single-threaded WASM runtime where `Send` futures are not
// required. The future is large because it holds merged `Gpapi` download
// state across several device targets; heap-allocating would add indirection
// for little benefit on this endpoint.
#[allow(clippy::future_not_send, clippy::large_futures)]
/// Fetch latest app details and merged download info.
///
/// # Errors
///
/// Returns a worker error if JSON serialization fails.
pub async fn get_download_info(
    package_name: String,
    channel: String,
    client_registry: SharedClientRegistry,
) -> Result<Response> {
    let channel = match Channel::from_str(&channel) {
        Ok(ch) => ch,
        Err(e) => return error_response(400, e),
    };

    let result = client_registry
        .get_download_info(&package_name, channel)
        .await;

    match result {
        Ok(Some((details, download_info))) => {
            let data = DownloadResponse {
                details: JsonDetails(details),
                download: DownloadInfo::from(download_info),
            };
            let response = ApiResponse {
                success: true,
                data: Some(data),
                error: None,
            };
            Ok(Response::from_json(&response)?)
        }
        Ok(None) => error_response(404, format!("App '{package_name}' not found")),
        Err(e) => error_response(500, e),
    }
}
