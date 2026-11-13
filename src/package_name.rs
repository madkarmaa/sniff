//! Package name validation, applied before any upstream call so malformed
//! input never reaches Google or the edge cache key.

use crate::api_error::ApiError;

/// Android's limit is far lower in practice; this only bounds hostile input.
pub const MAX_PACKAGE_NAME_LEN: usize = 255;

/// `[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+`, the same grammar the
/// Android app accepts: two or more dot-separated segments, each starting with
/// a letter.
#[must_use]
pub fn is_valid_package_name(name: &str) -> bool {
    name.len() <= MAX_PACKAGE_NAME_LEN
        && name.split('.').count() >= 2
        && name.split('.').all(is_valid_segment)
}

fn is_valid_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// # Errors
///
/// Returns an `InvalidRequest` error when `name` is not a valid package name.
pub fn validate_package_name(name: &str) -> Result<(), ApiError> {
    if is_valid_package_name(name) {
        Ok(())
    } else {
        // The input is not echoed: it is unbounded and attacker-controlled.
        Err(ApiError::invalid_request(format!(
            "Invalid package name: expected dot-separated segments of letters, digits and underscores, each starting with a letter (e.g. com.example.app), at most {MAX_PACKAGE_NAME_LEN} characters"
        )))
    }
}

#[cfg(test)]
#[path = "tests/package_name.rs"]
mod tests;
