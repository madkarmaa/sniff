//! The worker's error type: a kind, which decides the HTTP status, plus a
//! concise message that is safe to show to clients. Detailed diagnostics are
//! logged where the failure happens and never travel in the message.
//!
//! Nothing here touches the Workers runtime, so it is unit-testable natively.

use crate::google_play_client::Channel;
use crate::rate_limit;
use gpapi::error::{Error as GpapiError, ErrorKind as GpapiErrorKind, http_status};
use std::error::Error as StdError;
use std::fmt;

/// `Retry-After` sent with a 503. Google does not tell us how long to wait
/// (gpapi does not surface its header), so this is a conservative fixed hint.
pub const RATE_LIMIT_RETRY_AFTER_SECS: u32 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Malformed channel or package name (400).
    InvalidRequest,
    /// The channel's account is not enrolled in the app's testing program (400).
    ChannelUnavailable,
    /// Google Play does not know the app (404).
    NotFound,
    /// The app exists but cannot be downloaded: Google refused the purchase or
    /// delivery, typically a paid app or one not offered to the accounts and
    /// devices this service uses. The request itself is valid, so this is a
    /// 422 rather than a 403 (which would suggest the caller lacks permission).
    NotDownloadable,
    /// The requested channel's credentials are not set up on this deployment (501).
    NotConfigured,
    /// Google rate-limited us (503 with `Retry-After`).
    RateLimited,
    /// The client made too many uncached lookups (429 with `Retry-After`).
    TooManyRequests,
    /// Google failed or sent something unusable: non-2xx, network error,
    /// undecodable response, login failure (502).
    Upstream,
    /// Everything else, including server misconfiguration (500).
    Internal,
}

impl ErrorKind {
    #[must_use]
    pub const fn status(self) -> u16 {
        match self {
            Self::InvalidRequest | Self::ChannelUnavailable => 400,
            Self::NotFound => 404,
            Self::NotDownloadable => 422,
            Self::TooManyRequests => 429,
            Self::NotConfigured => 501,
            Self::RateLimited => 503,
            Self::Upstream => 502,
            Self::Internal => 500,
        }
    }
}

/// Which upstream step failed; decides how a gpapi error is classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Login,
    Details,
    Download,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    kind: ErrorKind,
    message: String,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl StdError for ApiError {}

impl ApiError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidRequest, message)
    }

    pub fn not_found(package_name: &str) -> Self {
        Self::new(
            ErrorKind::NotFound,
            format!("App '{package_name}' not found"),
        )
    }

    pub fn channel_unavailable(channel: Channel, package_name: &str) -> Self {
        Self::new(
            ErrorKind::ChannelUnavailable,
            format!("Channel '{channel}' is not available for package '{package_name}'"),
        )
    }

    pub fn rate_limited() -> Self {
        Self::new(
            ErrorKind::RateLimited,
            "Google Play is rate limiting this service. Try again shortly.",
        )
    }

    pub fn too_many_requests() -> Self {
        Self::new(
            ErrorKind::TooManyRequests,
            "Too many lookups from this address. Try again in a minute.",
        )
    }

    pub fn internal() -> Self {
        Self::new(ErrorKind::Internal, "Internal server error")
    }

    /// A required server setting is missing or unusable (500). The setting's
    /// name is logged by the caller, never returned.
    pub fn misconfigured() -> Self {
        Self::new(ErrorKind::Internal, "Server configuration error")
    }

    /// The credentials for `channel` are not set. Beta and alpha are optional
    /// per deployment (501); stable is required, so its absence is a
    /// misconfiguration (500).
    pub fn credentials_missing(channel: Channel) -> Self {
        match channel {
            Channel::Stable => Self::misconfigured(),
            Channel::Beta | Channel::Alpha => Self::new(
                ErrorKind::NotConfigured,
                format!("The {channel} channel is not configured on this server"),
            ),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub const fn status(&self) -> u16 {
        self.kind.status()
    }

    /// Seconds for the `Retry-After` header, for rate-limit errors.
    #[must_use]
    pub const fn retry_after_secs(&self) -> Option<u32> {
        match self.kind {
            ErrorKind::RateLimited => Some(RATE_LIMIT_RETRY_AFTER_SECS),
            ErrorKind::TooManyRequests => Some(rate_limit::PERIOD_SECS),
            _ => None,
        }
    }

    /// Classify an error from the Google Play client.
    ///
    /// The message is deliberately generic: `err`'s `Display` can carry
    /// response bodies and URLs, which belong in logs only.
    #[must_use]
    pub fn from_upstream(stage: Stage, channel: Channel, err: &(dyn StdError + 'static)) -> Self {
        if let Some(status) = http_status(err) {
            return if status == 429 {
                Self::rate_limited()
            } else if stage == Stage::Login {
                Self::new(
                    ErrorKind::Upstream,
                    format!("Google Play returned HTTP {status} while logging in"),
                )
            } else {
                Self::new(
                    ErrorKind::Upstream,
                    format!("Google Play returned HTTP {status}"),
                )
            };
        }
        let kind = err.downcast_ref::<GpapiError>().map(GpapiError::kind);
        match (kind, stage) {
            // The purchase/delivery flow answered without anything to download.
            (Some(GpapiErrorKind::InvalidApp), Stage::Download) => Self::new(
                ErrorKind::NotDownloadable,
                "This app cannot be downloaded: it may be paid or not available to the accounts and devices used by this service",
            ),
            // Only reachable through a bug: every call is preceded by a login.
            (Some(GpapiErrorKind::LoginRequired), _) => Self::internal(),
            (Some(GpapiErrorKind::Authentication), Stage::Login) => Self::new(
                ErrorKind::Upstream,
                format!("Google Play rejected the credentials of the {channel} channel"),
            ),
            (Some(GpapiErrorKind::TermsOfService), Stage::Login) => Self::new(
                ErrorKind::Upstream,
                format!(
                    "The account of the {channel} channel must accept the Google Play terms of service"
                ),
            ),
            (_, Stage::Login) => Self::new(
                ErrorKind::Upstream,
                format!("Could not log in to Google Play for the {channel} channel"),
            ),
            (_, Stage::Details | Stage::Download) => Self::new(
                ErrorKind::Upstream,
                format!("Google Play request failed for the {channel} channel"),
            ),
        }
    }

    /// The single error to report when every download target failed.
    ///
    /// `NotDownloadable` is reported only when all targets agree on it; any
    /// other failure is more likely transient, so it wins (rate limiting
    /// first, since it carries `Retry-After`).
    #[must_use]
    pub fn aggregate(errors: &[Self]) -> Self {
        errors
            .iter()
            .find(|error| error.kind() == ErrorKind::RateLimited)
            .or_else(|| {
                errors
                    .iter()
                    .find(|error| error.kind() != ErrorKind::NotDownloadable)
            })
            .or_else(|| errors.first())
            .cloned()
            .unwrap_or_else(Self::internal)
    }
}

#[cfg(test)]
#[path = "tests/api_error.rs"]
mod tests;
