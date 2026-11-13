//! Response construction shared by the router and the handlers: JSON
//! envelopes, error mapping, CORS and cache headers.

use crate::api_error::ApiError;
use crate::openapi_schema::ApiResponse;
use serde::Serialize;
use worker::{Response, Result, Url};

pub const CACHE_CONTROL_NO_STORE: &str = "no-store";

/// Static pages (docs, `OpenAPI` document) only change on deploy.
pub const CACHE_CONTROL_DOCS: &str = "public, max-age=300";

const ALLOWED_METHODS: &str = "GET, HEAD, OPTIONS";

/// How long a browser may reuse a preflight answer.
const PREFLIGHT_MAX_AGE: &str = "86400";

/// The JSON error envelope for `error`, never cached. A rate-limit error also
/// carries `Retry-After`.
///
/// # Errors
///
/// Returns a worker error if the response cannot be built.
pub fn error(error: &ApiError) -> Result<Response> {
    let response = Response::from_json(&ApiResponse::<()> {
        success: false,
        data: None,
        error: Some(error.message().to_string()),
    })?
    .with_status(error.status());
    let headers = response.headers();
    headers.set("Cache-Control", CACHE_CONTROL_NO_STORE)?;
    if let Some(retry_after) = error.retry_after_secs() {
        headers.set("Retry-After", &retry_after.to_string())?;
    }
    Ok(response)
}

/// A 200 JSON response with the given `Cache-Control`.
///
/// # Errors
///
/// Returns a worker error if serialization or header setting fails.
pub fn json_ok<T: Serialize>(body: &T, cache_control: &str) -> Result<Response> {
    let response = Response::from_json(body)?;
    response.headers().set("Cache-Control", cache_control)?;
    Ok(response)
}

/// A 200 text response (`content_type` given) with the given `Cache-Control`.
///
/// # Errors
///
/// Returns a worker error if the response cannot be built.
pub fn text_ok(body: &str, content_type: &str, cache_control: &str) -> Result<Response> {
    let response = Response::ok(body)?;
    let headers = response.headers();
    headers.set("Content-Type", content_type)?;
    headers.set("Cache-Control", cache_control)?;
    Ok(response)
}

/// A 302 to `url`.
///
/// Not `Response::redirect`: that wraps JavaScript's `Response.redirect()`,
/// whose headers are immutable, so `finalize` could not add the CORS headers
/// and every redirect failed with a 500.
///
/// # Errors
///
/// Returns a worker error if the response cannot be built.
pub fn redirect(url: &Url) -> Result<Response> {
    let response = Response::empty()?.with_status(302);
    response.headers().set("Location", url.as_str())?;
    Ok(response)
}

/// The answer to a CORS preflight for the API routes.
///
/// # Errors
///
/// Returns a worker error if the response cannot be built.
pub fn preflight() -> Result<Response> {
    let response = Response::empty()?.with_status(204);
    let headers = response.headers();
    headers.set("Access-Control-Allow-Methods", ALLOWED_METHODS)?;
    headers.set("Access-Control-Max-Age", PREFLIGHT_MAX_AGE)?;
    Ok(response)
}

/// Headers every response carries, whichever route produced it: open CORS
/// (the API is public and read-only), readable custom headers, and
/// `no-store` for any error that did not choose its own caching.
///
/// # Errors
///
/// Returns a worker error if a header cannot be set.
pub fn finalize(response: Response) -> Result<Response> {
    let headers = response.headers();
    headers.set("Access-Control-Allow-Origin", "*")?;
    headers.set(
        "Access-Control-Expose-Headers",
        "Retry-After, X-Sniff-Cache",
    )?;
    if response.status_code() >= 400 && !headers.has("Cache-Control")? {
        headers.set("Cache-Control", CACHE_CONTROL_NO_STORE)?;
    }
    Ok(response)
}
