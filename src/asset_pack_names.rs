//! The on-demand asset packs a client names in the `asset_packs` query
//! parameter. Google Play does not list these packs; only the app's code knows
//! their names.

use crate::api_error::ApiError;
use std::collections::BTreeSet;
use worker::Url;

pub const QUERY_PARAMETER: &str = "asset_packs";

/// Every name is sent to Google Play, so the list is kept short.
pub const MAX_ASSET_PACK_NAMES: usize = 20;

/// Bounds hostile input; real pack names are much shorter.
pub const MAX_ASSET_PACK_NAME_LEN: usize = 100;

/// The names in every `asset_packs` parameter of `url`, comma-separated,
/// sorted and without duplicates, so equal requests share a cache entry.
///
/// # Errors
///
/// Returns an `InvalidRequest` error for a malformed name or too many names.
pub fn requested_asset_packs(url: &Url) -> Result<Vec<String>, ApiError> {
    let names: BTreeSet<String> = url
        .query_pairs()
        .filter(|(key, _)| key == QUERY_PARAMETER)
        .flat_map(|(_, value)| {
            value
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .collect();
    if !names.iter().all(|name| is_valid_asset_pack_name(name)) {
        // The input is not echoed: it is unbounded and attacker-controlled.
        return Err(ApiError::invalid_request(format!(
            "Invalid asset pack name: expected letters, digits and underscores, at most {MAX_ASSET_PACK_NAME_LEN} characters"
        )));
    }
    if names.len() > MAX_ASSET_PACK_NAMES {
        return Err(ApiError::invalid_request(format!(
            "Too many asset packs: at most {MAX_ASSET_PACK_NAMES} can be requested"
        )));
    }
    Ok(names.into_iter().collect())
}

/// Pack names are module names, which bundletool limits to these characters.
fn is_valid_asset_pack_name(name: &str) -> bool {
    name.len() <= MAX_ASSET_PACK_NAME_LEN
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
#[path = "tests/asset_pack_names.rs"]
mod tests;
