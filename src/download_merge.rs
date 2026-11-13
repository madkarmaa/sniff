use crate::api_error::ApiError;
use gpapi::{DownloadInfo, NamedDownload};
use std::collections::HashSet;

/// A device profile and the ABI it is made to advertise to Google Play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub device: &'static str,
    pub abi: &'static str,
}

pub const DOWNLOAD_TARGETS: [Target; 4] = [
    Target {
        device: "px_9a",
        abi: "arm64-v8a",
    },
    Target {
        device: "sm_a13_5g",
        abi: "armeabi-v7a",
    },
    Target {
        device: "google_kiwi_x86_64",
        abi: "x86",
    },
    Target {
        device: "google_kiwi_x86_64",
        abi: "x86_64",
    },
];

/// Whether a download target contributed to the merged result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetOutcome {
    pub target: Target,
    /// Short client-safe message when the target failed.
    pub error: Option<String>,
}

impl TargetOutcome {
    #[must_use]
    pub const fn succeeded(&self) -> bool {
        self.error.is_none()
    }
}

/// Merge the targets that succeeded and report the outcome of every target.
/// Fails only when all of them failed, with the most telling error.
pub fn collect_targets(
    targets: &[Target],
    results: Vec<Result<DownloadInfo, ApiError>>,
) -> Result<(DownloadInfo, Vec<TargetOutcome>), ApiError> {
    let mut download_infos = Vec::new();
    let mut errors = Vec::new();
    let mut outcomes = Vec::new();
    for (target, result) in targets.iter().copied().zip(results) {
        match result {
            Ok(download_info) => {
                download_infos.push(download_info);
                outcomes.push(TargetOutcome {
                    target,
                    error: None,
                });
            }
            Err(error) => {
                outcomes.push(TargetOutcome {
                    target,
                    error: Some(error.message().to_string()),
                });
                errors.push(error);
            }
        }
    }

    if download_infos.is_empty() {
        Err(ApiError::aggregate(&errors))
    } else {
        Ok((merge_download_infos(download_infos), outcomes))
    }
}

#[must_use]
fn merge_download_infos(download_infos: Vec<DownloadInfo>) -> DownloadInfo {
    let mut merged = DownloadInfo::default();
    let mut split_names = HashSet::new();
    let mut additional_filenames = HashSet::new();

    for info in download_infos {
        // Size and hashes must come from the same info as the URL they describe.
        if merged.main_apk_url.is_none() && info.main_apk_url.is_some() {
            merged.main_apk_url = info.main_apk_url;
            merged.main_apk_size = info.main_apk_size;
            merged.main_apk_sha1 = info.main_apk_sha1;
            merged.main_apk_sha256 = info.main_apk_sha256;
        }
        merged.dex_metadata_url = merged.dex_metadata_url.or(info.dex_metadata_url);

        for split in info.splits {
            push_unique(&mut merged.splits, &mut split_names, split);
        }

        for file in info.additional_files {
            push_unique(
                &mut merged.additional_files,
                &mut additional_filenames,
                file,
            );
        }
    }

    merged
}

/// Push `item` unless one with the same name (or URL, when unnamed) was already pushed.
fn push_unique(target: &mut Vec<NamedDownload>, seen: &mut HashSet<String>, item: NamedDownload) {
    let key = item
        .name
        .as_ref()
        .or(item.download_url.as_ref())
        .cloned()
        .unwrap_or_default();
    if seen.insert(key) {
        target.push(item);
    }
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn)]
#[path = "tests/download_merge.rs"]
mod tests;
