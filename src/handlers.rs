use crate::api_error::ApiError;
use crate::client_registry::SharedClientRegistry;
use crate::edge_cache;
use crate::google_play_client::Channel;
use crate::openapi_schema::{
    ApiResponse, DownloadResponse, ErrorResponse, SerializableDetailsResponse,
};
use crate::package_name::validate_package_name;
use crate::responses;
use crate::shared_lookup;
use std::rc::Rc;
use std::str::FromStr;
use worker::{Context, RateLimiter, Response, Result, Url};

/// Per-request state shared with the route handlers.
#[derive(Clone)]
pub struct AppState {
    pub registry: SharedClientRegistry,
    pub ctx: Rc<Context>,
    /// Limits uncached lookups per client; `None` when not bound.
    pub limiter: Option<Rc<RateLimiter>>,
}

/// Who is asking, as far as the rate limit is concerned.
pub struct Caller {
    pub request_url: Url,
    /// `CF-Connecting-IP`; absent only outside Cloudflare's edge.
    pub address: Option<String>,
}

// The `path` literal must match `crate::API_PREFIX` (utoipa needs a literal);
// a unit test enforces it.
#[utoipa::path(
    get,
    path = "/v3/download/{package_name}/{channel}",
    params(
        ("package_name" = String, Path, description = "Android package name: dot-separated segments of letters, digits and underscores, each starting with a letter (at most 255 characters)", example = "com.discord"),
        ("channel" = String, Path, description = "Release channel (case-insensitive)", example = "stable")
    ),
    responses(
        (status = 200, description = "Latest app details and download info retrieved successfully. Served from a 120-second edge cache; see X-Sniff-Cache. Check data.targets: a lookup where only some targets succeeded still returns 200.",
         body = ApiResponse<DownloadResponse<SerializableDetailsResponse>>,
         headers(
             ("X-Sniff-Cache" = String, description = "HIT when served from the edge cache, MISS when freshly fetched"),
             ("Cache-Control" = String, description = "public, max-age=<seconds the response may still be reused>")
         ),
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
                 "main_apk_size": 12_345_678,
                 "main_apk_sha1": "yq3dhkoGu0Ub1SEcMRMyuP-g5Ps",
                 "main_apk_sha256": "xT8jM2k0g5Y9bQpQzC1m0KqB1G5w2h3o4vZ7nL8aRdE",
                 "splits": [
                     {
                         "name": "config.arm64_v8a",
                         "download_url": "https://play.googleapis.com/download/by-token/download?token=AOTCm0R...",
                         "size": 4_567_890,
                         "sha1": "aW1hZ2luYXJ5LXNoYTEtZGlnZXN0",
                         "sha256": "bm90LWEtcmVhbC1zaGEyNTYtZGlnZXN0LWp1c3QtYW4tZXhhbXBsZQ"
                     },
                     {
                         "name": "config.en",
                         "download_url": "https://play.googleapis.com/download/by-token/download?token=AOTCm0S...",
                         "size": 102_400,
                         "sha1": "ZXhhbXBsZS1zcGxpdC1zaGEx",
                         "sha256": "ZXhhbXBsZS1zcGxpdC1zaGEyNTYtZGlnZXN0LW5vdC1yZWFs"
                     }
                 ],
                 "additional_files": [],
                 "dex_metadata_url": null,
                 "targets": [
                     { "abi": "arm64-v8a", "device": "px_9a", "success": true, "error": null },
                     { "abi": "armeabi-v7a", "device": "sm_a13_5g", "success": true, "error": null }
                 ]
             },
             "error": null
         })
        ),
        (status = 400, description = "Invalid channel, invalid package name, or channel not available for the package",
         body = ErrorResponse,
         example = json!({
             "success": false,
             "data": null,
             "error": "Invalid Channel: snapshot"
         })
        ),
        (status = 404, description = "App not found. Served from a 60-second edge cache; see X-Sniff-Cache.",
         body = ErrorResponse,
         headers(
             ("X-Sniff-Cache" = String, description = "HIT when served from the edge cache, MISS when freshly fetched"),
             ("Cache-Control" = String, description = "public, max-age=<seconds the response may still be reused>")
         ),
         example = json!({
             "success": false,
             "data": null,
             "error": "App 'com.discord' not found"
         })
        ),
        (status = 422, description = "The app exists but cannot be downloaded: Google Play refused the purchase or delivery for every download target, typically a paid app or one not available to the accounts and devices this service uses. The request is valid, hence 422 rather than 403.",
         body = ErrorResponse,
         example = json!({
             "success": false,
             "data": null,
             "error": "This app cannot be downloaded: it may be paid or not available to the accounts and devices used by this service"
         })
        ),
        (status = 429, description = "Too many uncached lookups from this client address (IPv6: its /64 network); retry after the delay in Retry-After. Edge-cache hits and requests that join a lookup already in progress are never limited.",
         body = ErrorResponse,
         headers(
             ("Retry-After" = u32, description = "Seconds to wait before retrying")
         ),
         example = json!({
             "success": false,
             "data": null,
             "error": "Too many lookups from this address. Try again in a minute."
         })
        ),
        (status = 500, description = "Internal server error, including server misconfiguration",
         body = ErrorResponse,
         example = json!({
             "success": false,
             "data": null,
             "error": "Internal server error"
         })
        ),
        (status = 501, description = "The requested channel is not configured on this deployment",
         body = ErrorResponse,
         example = json!({
             "success": false,
             "data": null,
             "error": "The beta channel is not configured on this server"
         })
        ),
        (status = 502, description = "Google Play failed: a non-2xx answer, a network error, an unusable response or a login failure",
         body = ErrorResponse,
         example = json!({
             "success": false,
             "data": null,
             "error": "Google Play returned HTTP 500"
         })
        ),
        (status = 503, description = "Google Play is rate limiting this service; retry after the delay in Retry-After",
         body = ErrorResponse,
         headers(
             ("Retry-After" = u32, description = "Seconds to wait before retrying")
         ),
         example = json!({
             "success": false,
             "data": null,
             "error": "Google Play is rate limiting this service. Try again shortly."
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
/// Concurrent requests for the same uncached app share one lookup (see
/// `shared_lookup`). Lookup failures are returned as error responses, so this only fails when
/// building a response does.
///
/// # Errors
///
/// Returns a worker error if JSON serialization or header setting fails.
pub async fn get_download_info(
    package_name: String,
    channel: String,
    caller: Caller,
    app: AppState,
) -> Result<Response> {
    let channel = match Channel::from_str(&channel) {
        Ok(channel) => channel,
        Err(message) => return responses::error(&ApiError::invalid_request(message)),
    };
    if let Err(error) = validate_package_name(&package_name) {
        return responses::error(&error);
    }

    // Keyed by the validated inputs, so a hit never reaches Google.
    let cache_key = edge_cache::cache_key(&caller.request_url, &package_name, channel);
    if let Some(hit) = edge_cache::lookup(&cache_key).await {
        return Ok(hit);
    }

    shared_lookup::respond(cache_key, &package_name, channel, &caller, &app).await
}
