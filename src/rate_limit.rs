//! Per-client limit on lookups that reach Google Play.
//!
//! Edge-cache hits and requests that join a lookup already in progress cost
//! nothing and are never counted. A miss costs about five upstream calls on
//! the operator's accounts, so one client looping over the API could get
//! every account rate limited (or flagged) for everyone. The Workers
//! rate-limit binding counts misses per client address in each
//! Cloudflare location, with the limit set in `wrangler.toml`.
//!
//! Without the binding (local development), or when it errors, lookups go
//! through. Refusing every request would be worse than briefly not limiting
//! them.

use std::net::IpAddr;
use worker::{Env, RateLimiter, console_error, console_warn};

/// The rate-limit binding.
pub const BINDING: &str = "LOOKUP_LIMITER";

/// The binding's `simple.period`, sent as `Retry-After` when a client is over
/// the limit. By then its window has rolled over.
pub const PERIOD_SECS: u32 = 60;

/// The key a client is counted under. That is its address, or for IPv6 its
/// /64 network, since one subscriber usually holds a whole /64 and could
/// otherwise rotate addresses to dodge the limit.
#[must_use]
pub fn client_key(address: &str) -> Option<String> {
    let key = match address.trim().parse::<IpAddr>().ok()?.to_canonical() {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => {
            let [a, b, c, d, ..] = v6.segments();
            format!("{a:x}:{b:x}:{c:x}:{d:x}::/64")
        }
    };
    Some(key)
}

/// The rate limiter, or `None` (logged) when this deployment has none.
#[must_use]
pub fn open(env: &Env) -> Option<RateLimiter> {
    match env.rate_limiter(BINDING) {
        Ok(limiter) => Some(limiter),
        Err(e) => {
            console_warn!(
                "Rate limit binding {BINDING} is unavailable, lookups are unlimited: {e}"
            );
            None
        }
    }
}

/// Whether the client at `address` may make another lookup. Counts it.
pub async fn allow(limiter: Option<&RateLimiter>, address: Option<&str>) -> bool {
    let (Some(limiter), Some(key)) = (limiter, address.and_then(client_key)) else {
        return true;
    };
    match limiter.limit(key).await {
        Ok(outcome) => outcome.success,
        Err(e) => {
            console_error!("Rate limiter failed, allowing the lookup: {e}");
            true
        }
    }
}

#[cfg(test)]
#[path = "tests/rate_limit.rs"]
mod tests;
