use super::*;
use crate::api_error::ErrorKind;

fn named(name: &str, url: &str) -> NamedDownload {
    NamedDownload {
        name: Some(name.into()),
        download_url: Some(url.into()),
        ..Default::default()
    }
}

#[test]
fn merge_takes_main_apk_metadata_from_the_info_that_supplied_the_url() {
    let without_main = DownloadInfo {
        main_apk_size: Some(1),
        main_apk_sha1: Some("stray".into()),
        ..Default::default()
    };
    let with_main = DownloadInfo {
        main_apk_url: Some("armv7/base".into()),
        main_apk_size: Some(32),
        main_apk_sha1: Some("armv7-sha1".into()),
        ..Default::default()
    };
    let merged = merge_download_infos(vec![without_main, with_main]);
    assert_eq!(merged.main_apk_url.as_deref(), Some("armv7/base"));
    assert_eq!(merged.main_apk_size, Some(32));
    assert_eq!(merged.main_apk_sha1.as_deref(), Some("armv7-sha1"));
    assert_eq!(merged.main_apk_sha256, None);
}

#[test]
fn merge_keeps_first_urls_and_deduplicates_by_name() {
    let first = DownloadInfo {
        main_apk_url: Some("arm64/base".into()),
        main_apk_size: Some(64),
        main_apk_sha1: Some("arm64-sha1".into()),
        main_apk_sha256: Some("arm64-sha256".into()),
        splits: vec![
            named("config.arm64_v8a", "arm64/abi"),
            named("config.en", "arm64/en"),
        ],
        additional_files: vec![named("main.1.app.obb", "arm64/obb")],
        dex_metadata_url: None,
    };
    let second = DownloadInfo {
        main_apk_url: Some("armv7/base".into()),
        main_apk_size: Some(32),
        main_apk_sha1: Some("armv7-sha1".into()),
        main_apk_sha256: Some("armv7-sha256".into()),
        splits: vec![
            named("config.armeabi_v7a", "armv7/abi"),
            named("config.en", "armv7/en"),
        ],
        additional_files: vec![named("main.1.app.obb", "armv7/obb")],
        dex_metadata_url: Some("armv7/dm".into()),
    };
    let merged = merge_download_infos(vec![first, second]);
    assert_eq!(merged.main_apk_url.as_deref(), Some("arm64/base"));
    assert_eq!(merged.main_apk_size, Some(64));
    assert_eq!(merged.main_apk_sha1.as_deref(), Some("arm64-sha1"));
    assert_eq!(merged.main_apk_sha256.as_deref(), Some("arm64-sha256"));
    assert_eq!(merged.dex_metadata_url.as_deref(), Some("armv7/dm"));
    let split_urls: Vec<_> = merged
        .splits
        .iter()
        .filter_map(|split| split.download_url.as_deref())
        .collect();
    assert_eq!(split_urls, ["arm64/abi", "arm64/en", "armv7/abi"]);
    assert_eq!(merged.additional_files.len(), 1);
}

fn info(main: &str) -> DownloadInfo {
    DownloadInfo {
        main_apk_url: Some(main.into()),
        splits: vec![named(&format!("config.{main}"), main)],
        ..Default::default()
    }
}

fn failure(kind: ErrorKind, message: &str) -> ApiError {
    ApiError::new(kind, message)
}

#[test]
fn download_targets_cover_every_abi_once() {
    let abis: Vec<_> = DOWNLOAD_TARGETS.iter().map(|target| target.abi).collect();
    assert_eq!(abis, ["arm64-v8a", "armeabi-v7a"]);
}

#[test]
fn collect_targets_reports_every_target_when_all_succeed() -> Result<(), ApiError> {
    let results = vec![Ok(info("a")), Ok(info("b"))];
    let (merged, outcomes) = collect_targets(&DOWNLOAD_TARGETS, results)?;
    assert_eq!(merged.splits.len(), 2);
    assert_eq!(outcomes.len(), DOWNLOAD_TARGETS.len());
    for (outcome, target) in outcomes.iter().zip(DOWNLOAD_TARGETS) {
        assert_eq!(outcome.target, target);
        assert!(outcome.succeeded());
        assert_eq!(outcome.error, None);
    }
    Ok(())
}

#[test]
fn collect_targets_flags_partial_merges() -> Result<(), ApiError> {
    let results = vec![
        Ok(info("a")),
        Err(failure(
            ErrorKind::Upstream,
            "Google Play returned HTTP 500",
        )),
    ];
    let (merged, outcomes) = collect_targets(&DOWNLOAD_TARGETS, results)?;
    // Only the successful targets are merged...
    let split_names: Vec<_> = merged
        .splits
        .iter()
        .filter_map(|split| split.name.as_deref())
        .collect();
    assert_eq!(split_names, ["config.a"]);
    // ...and the failed ones stay visible, in target order.
    let summary: Vec<_> = outcomes
        .iter()
        .map(|outcome| {
            (
                outcome.target.abi,
                outcome.succeeded(),
                outcome.error.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("arm64-v8a", true, None),
            ("armeabi-v7a", false, Some("Google Play returned HTTP 500")),
        ]
    );
    Ok(())
}

#[test]
fn collect_targets_fails_only_when_every_target_failed() {
    let results = vec![
        Err(failure(ErrorKind::NotDownloadable, "no")),
        Err(failure(ErrorKind::NotDownloadable, "no")),
    ];
    assert_eq!(
        collect_targets(&DOWNLOAD_TARGETS, results)
            .err()
            .map(|e| e.status()),
        Some(422)
    );

    let results = vec![
        Err(failure(ErrorKind::NotDownloadable, "no")),
        Err(failure(ErrorKind::RateLimited, "slow down")),
    ];
    assert_eq!(
        collect_targets(&DOWNLOAD_TARGETS, results)
            .err()
            .map(|e| e.status()),
        Some(503)
    );
}
