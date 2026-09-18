#![allow(clippy::await_holding_lock)]

use crate::client_registry::SharedClientRegistry;
use crate::google_play_client::Channel;
use crate::openapi_schema::{
    ApiResponse, DownloadInfo, ErrorResponse, MultiChannelApiResponse, SerializableDetailsResponse,
};
use crate::serializable_types::SerializableDetailsResponse as JsonDetails;
use std::collections::HashMap;
use std::str::FromStr;
use worker::{Headers, Response, Result};

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
    path = "/v1/details/{package_name}",
    params(
        ("package_name" = String, Path, description = "Android package name (e.g., com.discord)")
    ),
    responses(
        (status = 200, description = "App details retrieved successfully",
         body = MultiChannelApiResponse<SerializableDetailsResponse>,
         headers(
             ("X-Available-Channels" = String, description = "Comma-separated list of available channels")
         ),
         example = json!({
             "success": true,
             "data": {
                 "stable": {
                     "item": {
                         "id": "com.discord",
                         "type": 1,
                         "title": "Discord - Talk, Play, Hang Out",
                         "creator": "Discord Inc.",
                         "details": {
                             "app_details": {
                                 "developer_name": "Discord Inc.",
                                 "version_code": 289_020,
                                 "version_string": "289.20 - Stable",
                                 "package_name": "com.discord"
                             }
                         }
                     },
                     "footer_html": "All prices include VAT.",
                     "enable_reviews": true
                 }
             },
             "error": null
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
             "error": "Login error for stable channel: Invalid app response"
         })
        )
    ),
    tag = "App Details"
)]
// Workers run on a single-threaded WASM runtime where `Send` futures are not
// required; the registry holds `worker::Env` (`JsValue`) which is `!Send`.
#[allow(clippy::future_not_send)]
/// Fetch details across all channels.
///
/// # Errors
///
/// Returns a worker error if JSON serialization or header manipulation fails.
pub async fn get_details_multi(
    package_name: String,
    client_registry: SharedClientRegistry,
) -> Result<Response> {
    match client_registry
        .lock()
        .await
        .get_details_multi(&package_name)
        .await
    {
        Ok(Some(details_map)) => {
            let serialized_map: HashMap<String, JsonDetails> = details_map
                .into_iter()
                .map(|(channel, details)| (channel.to_string(), JsonDetails(details)))
                .collect();

            let available_channels = serialized_map.keys().cloned().collect::<Vec<_>>().join(",");

            let response = MultiChannelApiResponse {
                success: true,
                data: Some(serialized_map),
                error: None,
            };

            let headers = Headers::new();
            headers.set("Content-Type", "application/json")?;
            headers.set("X-Available-Channels", &available_channels)?;

            Ok(Response::from_json(&response)?.with_headers(headers))
        }
        Ok(None) => error_response(404, format!("App '{package_name}' not found")),
        Err(e) => error_response(500, e),
    }
}

#[utoipa::path(
    get,
    path = "/v1/details/{package_name}/{channel}",
    params(
        ("package_name" = String, Path, description = "Android package name (e.g., com.discord)"),
        ("channel" = String, Path, description = "Release channel", example = "stable")
    ),
    responses(
        (status = 200, description = "App details retrieved successfully",
         body = ApiResponse<SerializableDetailsResponse>,
         example = json!({
             "success": true,
             "data": {
                 "item": {
                     "id": "com.discord",
                     "type": 1,
                     "title": "Discord - Talk, Play, Hang Out",
                     "creator": "Discord Inc.",
                     "details": {
                         "app_details": {
                             "developer_name": "Discord Inc.",
                             "version_code": 289_020,
                             "version_string": "289.20 - Stable",
                             "package_name": "com.discord"
                         }
                     }
                 },
                 "footer_html": "All prices include VAT.",
                 "enable_reviews": true
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
    tag = "App Details"
)]
// Workers run on a single-threaded WASM runtime where `Send` futures are not
// required; the registry holds `worker::Env` (`JsValue`) which is `!Send`.
#[allow(clippy::future_not_send)]
/// Fetch details for a single channel.
///
/// # Errors
///
/// Returns a worker error if JSON serialization fails.
pub async fn get_details_single(
    package_name: String,
    channel: String,
    client_registry: SharedClientRegistry,
) -> Result<Response> {
    let channel = match Channel::from_str(&channel) {
        Ok(ch) => ch,
        Err(e) => return error_response(400, e),
    };

    let result = client_registry
        .lock()
        .await
        .get_details_with_fallback(&package_name, channel)
        .await;

    match result {
        Ok(Some((_, details))) => {
            let response = ApiResponse {
                success: true,
                data: Some(JsonDetails(details)),
                error: None,
            };
            Ok(Response::from_json(&response)?)
        }
        Ok(None) => error_response(404, format!("App '{package_name}' not found")),
        Err(e) => error_response(500, e),
    }
}

#[utoipa::path(
    get,
    path = "/v1/download/{package_name}/{channel}/{version_code}",
    params(
        ("package_name" = String, Path, description = "Android package name"),
        ("channel" = String, Path, description = "Release channel"),
        ("version_code" = i32, Path, description = "Android version code")
    ),
    responses(
        (status = 200, description = "Download info retrieved successfully",
         body = ApiResponse<DownloadInfo>,
         example = json!({
             "success": true,
             "data": {
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
        (status = 400, description = "Invalid parameters",
         body = ErrorResponse,
         example = json!({
             "success": false,
             "data": null,
             "error": "Invalid Channel: snapshot"
         })
        ),
        (status = 404, description = "App or version not found",
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
/// Fetch merged download info.
///
/// # Errors
///
/// Returns a worker error if JSON serialization fails.
pub async fn get_download_info(
    package_name: String,
    channel: String,
    version_code: i64,
    client_registry: SharedClientRegistry,
) -> Result<Response> {
    let channel = match Channel::from_str(&channel) {
        Ok(ch) => ch,
        Err(e) => return error_response(400, e),
    };

    let result = client_registry
        .lock()
        .await
        .get_download_info(&package_name, channel, Some(version_code))
        .await;

    match result {
        Ok(Some((_, download_info))) => {
            let openapi_download_info = DownloadInfo::from(download_info);
            let response = ApiResponse {
                success: true,
                data: Some(openapi_download_info),
                error: None,
            };
            Ok(Response::from_json(&response)?)
        }
        Ok(None) => error_response(404, format!("App '{package_name}' not found")),
        Err(e) => error_response(500, e),
    }
}
