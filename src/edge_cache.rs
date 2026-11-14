//! Short-lived edge cache for lookups, via the Workers Cache API.
//!
//! One lookup costs about five upstream calls against the operator's Google
//! accounts, so identical requests within a few seconds share one result.
//! Successful lookups and "app not found" answers are cached; other errors
//! are usually transient and are not.
//! Every cache failure is logged and ignored: the Cache API is a no-op or
//! unavailable on `*.workers.dev` and in some local-dev modes, and it must
//! never turn a good lookup into an error.

use crate::API_PREFIX;
use crate::google_play_client::Channel;
use worker::{Cache, Context, Response, Url, console_error};

/// How long a successful download response is served from the edge cache.
///
/// Trade-off: the signed download URLs in the response expire and a new app
/// version should show up quickly, so this must stay short; but each miss
/// costs ~5 upstream calls, so even a couple of minutes of sharing spares the
/// accounts from bursts (retries, several clients asking for a trending app).
/// 120 seconds keeps URLs far inside their lifetime while still absorbing
/// those bursts.
pub const TTL_SECS: u64 = 120;

/// How long an "app not found" answer is served from the edge cache.
///
/// Kept brief so an app that gets published shows up soon, while a client
/// polling for a missing app no longer costs a details call each time.
pub const NOT_FOUND_TTL_SECS: u64 = 60;

/// How long a response with `status` may be cached, or `None` when it must
/// not be.
#[must_use]
pub const fn ttl_secs(status: u16) -> Option<u64> {
    match status {
        200 => Some(TTL_SECS),
        404 => Some(NOT_FOUND_TTL_SECS),
        _ => None,
    }
}

/// Canonical cache key for a download lookup.
///
/// Built from the validated inputs rather than the raw request URL: the
/// channel is lower-cased and the query string and fragment are dropped, so
/// trivially different URLs share one entry. The package name is
/// case-sensitive on Google Play and is kept as is.
#[must_use]
pub fn cache_key(request_url: &Url, package_name: &str, channel: Channel) -> String {
    let mut url = request_url.clone();
    url.set_path(&format!("{API_PREFIX}/download/{package_name}/{channel}"));
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

/// `Cache-Control` for a stored response, which the Cache API honours.
#[must_use]
pub fn cache_control(max_age_secs: u64) -> String {
    format!("public, max-age={max_age_secs}")
}

/// Seconds a cached response stored for `ttl_secs` has left, given its `Age`
/// header (how long the edge has held it).
#[must_use]
pub fn remaining_ttl_secs(ttl_secs: u64, age_header: Option<&str>) -> u64 {
    let age = age_header
        .and_then(|age| age.trim().parse::<u64>().ok())
        .unwrap_or(0);
    ttl_secs.saturating_sub(age)
}

/// Look `key` up. Returns a fresh response (never the cached object, whose
/// headers may be immutable) marked `X-Sniff-Cache: HIT`.
pub async fn lookup(key: &str) -> Option<Response> {
    let mut hit = match Cache::default().get(key, false).await {
        Ok(Some(hit)) => hit,
        Ok(None) => return None,
        Err(e) => {
            console_error!("Edge cache lookup failed: {e}");
            return None;
        }
    };
    let status = hit.status_code();
    let age = hit.headers().get("Age").ok().flatten();
    let bytes = match hit.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => {
            console_error!("Edge cache hit could not be read: {e}");
            return None;
        }
    };
    match rebuild(bytes, status, age.as_deref()) {
        Ok(response) => Some(response),
        Err(e) => {
            console_error!("Edge cache hit could not be rebuilt: {e}");
            None
        }
    }
}

fn rebuild(bytes: Vec<u8>, status: u16, age_header: Option<&str>) -> worker::Result<Response> {
    // Only cacheable statuses are stored, so the fallback is never used.
    let ttl_secs = ttl_secs(status).unwrap_or(0);
    let response = Response::from_bytes(bytes)?.with_status(status);
    let headers = response.headers();
    headers.set("Content-Type", "application/json")?;
    headers.set(
        "Cache-Control",
        &cache_control(remaining_ttl_secs(ttl_secs, age_header)),
    )?;
    headers.set("X-Sniff-Cache", "HIT")?;
    Ok(response)
}

/// Store `response` under `key` without delaying the reply: the put runs
/// under `wait_until`, which keeps the invocation alive until it finishes.
pub fn store(ctx: &Context, key: String, response: Response) {
    ctx.wait_until(async move {
        if let Err(e) = Cache::default().put(key, response).await {
            console_error!("Edge cache store failed: {e}");
        }
    });
}

#[cfg(test)]
// Tests use assertions for contract failures and `?` for parse errors.
#[allow(clippy::panic_in_result_fn)]
#[path = "tests/edge_cache.rs"]
mod tests;
