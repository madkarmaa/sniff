#![allow(clippy::await_holding_lock)]

use crate::client_registry::SharedClientRegistry;
use crate::google_play_client::Channel;
use crate::openapi_schema::{
    ApiResponse, DownloadInfo, MultiChannelApiResponse, SerializableDetailsResponse,
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
         )
        ),
        (status = 404, description = "App not found", body = MultiChannelApiResponse<SerializableDetailsResponse>),
        (status = 500, description = "Internal server error", body = MultiChannelApiResponse<SerializableDetailsResponse>)
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
        Ok(details_map) => {
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
        (status = 200, description = "App details retrieved successfully", body = ApiResponse<SerializableDetailsResponse>),
        (status = 400, description = "Invalid channel", body = ApiResponse<String>),
        (status = 404, description = "App not found", body = ApiResponse<SerializableDetailsResponse>),
        (status = 500, description = "Internal server error", body = ApiResponse<SerializableDetailsResponse>)
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
        (status = 200, description = "Download info retrieved successfully", body = ApiResponse<DownloadInfo>),
        (status = 400, description = "Invalid parameters", body = ApiResponse<String>),
        (status = 404, description = "App or version not found", body = ApiResponse<DownloadInfo>),
        (status = 500, description = "Internal server error", body = ApiResponse<DownloadInfo>)
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
