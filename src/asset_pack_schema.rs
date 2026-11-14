//! How the response describes asset packs.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// When Google Play delivers an asset pack to an installed app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryMode {
    /// Right after the app is installed.
    FastFollow,
    /// When the app asks for it.
    OnDemand,
}

/// One APK of a Play Asset Delivery pack.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct AssetPackFile {
    /// Name of the asset pack.
    #[schema(example = "krisp")]
    pub name: String,
    /// Identifies this APK within its pack. A pack can come in several APKs,
    /// and devices with different texture formats can get different ones.
    #[schema(example = "krisp")]
    pub slice_id: Option<String>,
    pub delivery_mode: DeliveryMode,
    /// Version of the asset pack, usually the app's version code.
    #[schema(example = 353_011)]
    pub version_code: Option<i64>,
    #[schema(example = "https://play.googleapis.com/download/by-token/download?token=AOTCm0U...")]
    pub download_url: Option<String>,
    /// Size in bytes.
    #[schema(example = 12_202_642)]
    pub size: Option<i64>,
    /// SHA-256 exactly as Google Play reports it (typically base64url, not hex).
    #[schema(example = "bm90LWEtcmVhbC1hc3NldC1wYWNrLXNoYTI1Ni1kaWdlc3Q")]
    pub sha256: Option<String>,
}

/// The response form of `files`. Packs not in `fast_follow_packs` are
/// on-demand.
#[must_use]
pub fn asset_pack_files(
    files: Vec<gpapi::AssetPackFile>,
    fast_follow_packs: &[String],
) -> Vec<AssetPackFile> {
    files
        .into_iter()
        .map(|file| AssetPackFile {
            delivery_mode: if fast_follow_packs.contains(&file.pack_name) {
                DeliveryMode::FastFollow
            } else {
                DeliveryMode::OnDemand
            },
            name: file.pack_name,
            slice_id: file.slice_id,
            version_code: file.version_code,
            download_url: file.download_url,
            size: file.size,
            sha256: file.sha256,
        })
        .collect()
}

#[cfg(test)]
// Tests use assertions for contract failures and `?` for serialization errors.
#[allow(clippy::panic_in_result_fn)]
#[path = "tests/asset_pack_schema.rs"]
mod tests;
