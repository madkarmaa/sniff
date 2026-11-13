use super::{ApiDoc, ApiResponse, DownloadInfo, DownloadResponse, TargetReport};
use crate::download_merge::{Target, TargetOutcome};
use crate::serializable_types::SerializableDetailsResponse;
use googleplay_protobuf::{AppDetails, DetailsResponse, DocumentDetails, Item};
use serde_json::{Value, json};
use utoipa::OpenApi;

fn details() -> DetailsResponse {
    DetailsResponse {
        item: Some(Item {
            id: Some("com.example".into()),
            title: Some("Example".into()),
            details: Some(DocumentDetails {
                app_details: Some(AppDetails {
                    version_code: Some(123),
                    version_string: Some("1.2.3".into()),
                    recent_changes_html: Some("Latest changes".into()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }),
        footer_html: Some("Footer".into()),
        enable_reviews: Some(false),
        ..Default::default()
    }
}

fn outcome(abi: &'static str, error: Option<&str>) -> TargetOutcome {
    TargetOutcome {
        target: Target {
            device: "px_9a",
            abi,
        },
        error: error.map(String::from),
    }
}

#[test]
fn download_response_preserves_metadata_and_downloads() -> Result<(), serde_json::Error> {
    let details = details();
    let metadata = serde_json::to_value(SerializableDetailsResponse(details.clone()))?;
    let download = DownloadInfo::from(gpapi::DownloadInfo {
        main_apk_url: Some("https://example.com/main.apk".into()),
        splits: vec![gpapi::NamedDownload {
            name: Some("config.arm64_v8a".into()),
            download_url: Some("https://example.com/arm64.apk".into()),
            ..Default::default()
        }],
        additional_files: vec![gpapi::NamedDownload {
            name: Some("main.123.com.example.obb".into()),
            download_url: Some("https://example.com/main.obb".into()),
            ..Default::default()
        }],
        dex_metadata_url: Some("https://example.com/main.dm".into()),
        ..Default::default()
    });
    let urls = serde_json::to_value(&download)?;
    let response = serde_json::to_value(ApiResponse {
        success: true,
        data: Some(DownloadResponse {
            details: SerializableDetailsResponse(details),
            download,
            targets: Vec::new(),
        }),
        error: None,
    })?;
    let mut expected_data = metadata;
    if let (Some(data), Some(urls)) = (expected_data.as_object_mut(), urls.as_object()) {
        data.extend(urls.clone());
        data.insert("targets".into(), json!([]));
    }
    assert_eq!(
        response,
        json!({"success": true, "data": expected_data, "error": null})
    );
    assert_eq!(
        response.pointer("/data/enable_reviews"),
        Some(&json!(false))
    );
    assert!(response.pointer("/data/item/creator").is_none());
    Ok(())
}

#[test]
fn download_info_serializes_sizes_and_hashes_as_reported() -> Result<(), serde_json::Error> {
    let download = DownloadInfo::from(gpapi::DownloadInfo {
        main_apk_url: Some("https://example.com/main.apk".into()),
        main_apk_size: Some(12_345_678),
        main_apk_sha1: Some("yq3dhkoGu0Ub1SEcMRMyuP-g5Ps".into()),
        main_apk_sha256: Some("main-sha256".into()),
        splits: vec![
            gpapi::NamedDownload {
                name: Some("config.en".into()),
                download_url: Some("https://example.com/en.apk".into()),
                size: Some(1024),
                sha1: Some("split-sha1".into()),
                sha256: Some("split-sha256".into()),
            },
            gpapi::NamedDownload {
                name: Some("config.it".into()),
                download_url: Some("https://example.com/it.apk".into()),
                ..Default::default()
            },
        ],
        additional_files: vec![gpapi::NamedDownload {
            name: Some("main.1.com.example.obb".into()),
            download_url: Some("https://example.com/main.obb".into()),
            size: Some(52_428_800),
            sha1: Some("obb-sha1".into()),
            sha256: None,
        }],
        dex_metadata_url: None,
    });
    let json = serde_json::to_value(&download)?;
    assert_eq!(
        json,
        json!({
            "main_apk_url": "https://example.com/main.apk",
            "main_apk_size": 12_345_678,
            // Hashes are passed through untouched, not re-encoded.
            "main_apk_sha1": "yq3dhkoGu0Ub1SEcMRMyuP-g5Ps",
            "main_apk_sha256": "main-sha256",
            "splits": [
                {
                    "name": "config.en",
                    "download_url": "https://example.com/en.apk",
                    "size": 1024,
                    "sha1": "split-sha1",
                    "sha256": "split-sha256"
                },
                // Absent values are `null`, like `dex_metadata_url`.
                {
                    "name": "config.it",
                    "download_url": "https://example.com/it.apk",
                    "size": null,
                    "sha1": null,
                    "sha256": null
                }
            ],
            "additional_files": [{
                "filename": "main.1.com.example.obb",
                "download_url": "https://example.com/main.obb",
                "size": 52_428_800,
                "sha1": "obb-sha1",
                "sha256": null
            }],
            "dex_metadata_url": null
        })
    );
    Ok(())
}

#[test]
fn missing_main_apk_metadata_serializes_as_null() -> Result<(), serde_json::Error> {
    let json = serde_json::to_value(DownloadInfo::from(gpapi::DownloadInfo::default()))?;
    for field in [
        "main_apk_url",
        "main_apk_size",
        "main_apk_sha1",
        "main_apk_sha256",
        "dex_metadata_url",
    ] {
        assert_eq!(json.get(field), Some(&Value::Null), "{field}");
    }
    Ok(())
}

#[test]
fn target_reports_show_successes_and_failures() -> Result<(), serde_json::Error> {
    let reports: Vec<TargetReport> = [
        outcome("arm64-v8a", None),
        outcome("x86", Some("Google Play returned HTTP 500")),
    ]
    .iter()
    .map(TargetReport::from)
    .collect();
    assert_eq!(
        serde_json::to_value(&reports)?,
        json!([
            {"abi": "arm64-v8a", "device": "px_9a", "success": true, "error": null},
            {
                "abi": "x86",
                "device": "px_9a",
                "success": false,
                "error": "Google Play returned HTTP 500"
            }
        ])
    );
    Ok(())
}

#[test]
fn download_response_carries_targets_next_to_the_downloads() -> Result<(), serde_json::Error> {
    let response = serde_json::to_value(DownloadResponse {
        details: SerializableDetailsResponse(details()),
        download: DownloadInfo::from(gpapi::DownloadInfo::default()),
        targets: vec![TargetReport::from(&outcome("x86_64", None))],
    })?;
    assert_eq!(response.pointer("/targets/0/abi"), Some(&json!("x86_64")));
    assert_eq!(response.pointer("/targets/0/success"), Some(&json!(true)));
    assert!(response.pointer("/main_apk_size").is_some());
    Ok(())
}

#[test]
fn openapi_exposes_only_latest_download_under_the_api_prefix() -> Result<(), serde_json::Error> {
    let spec = serde_json::to_value(ApiDoc::openapi())?;
    assert_eq!(
        spec.pointer("/info/version"),
        Some(&json!(env!("CARGO_PKG_VERSION")))
    );
    let paths = spec.get("paths").and_then(Value::as_object);
    assert_eq!(paths.map(serde_json::Map::len), Some(1));
    // The utoipa attribute needs a literal, so check it matches the router's prefix.
    for path in paths.into_iter().flat_map(serde_json::Map::keys) {
        assert!(
            path.starts_with(&format!("{}/", crate::API_PREFIX)),
            "{path} is outside {}",
            crate::API_PREFIX
        );
    }
    let operation = paths
        .and_then(|paths| paths.get("/v3/download/{package_name}/{channel}"))
        .and_then(|path| path.get("get"));
    let parameters = operation
        .and_then(|operation| operation.get("parameters"))
        .and_then(Value::as_array);
    let names = parameters.map(|parameters| {
        parameters
            .iter()
            .filter_map(|parameter| parameter.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>()
    });
    assert_eq!(names, Some(vec!["package_name", "channel"]));
    Ok(())
}

#[test]
fn openapi_documents_every_status_code_the_worker_can_return() -> Result<(), serde_json::Error> {
    let spec = serde_json::to_value(ApiDoc::openapi())?;
    let responses = spec
        .pointer("/paths/~1v3~1download~1{package_name}~1{channel}/get/responses")
        .and_then(Value::as_object);
    let mut documented: Vec<_> = responses
        .into_iter()
        .flat_map(serde_json::Map::keys)
        .map(String::as_str)
        .collect();
    documented.sort_unstable();
    assert_eq!(
        documented,
        [
            "200", "400", "404", "422", "429", "500", "501", "502", "503"
        ]
    );
    for status in ["429", "503"] {
        assert!(
            spec.pointer(&format!(
                "/paths/~1v3~1download~1{{package_name}}~1{{channel}}/get/responses/{status}/headers/Retry-After"
            ))
            .is_some(),
            "{status}"
        );
    }
    Ok(())
}

#[test]
fn openapi_describes_the_new_download_fields() -> Result<(), serde_json::Error> {
    let spec = serde_json::to_value(ApiDoc::openapi())?;
    let schemas = spec.pointer("/components/schemas");
    for (schema, field) in [
        ("DownloadInfo", "main_apk_size"),
        ("DownloadInfo", "main_apk_sha1"),
        ("DownloadInfo", "main_apk_sha256"),
        ("SplitFile", "size"),
        ("SplitFile", "sha1"),
        ("SplitFile", "sha256"),
        ("AdditionalFile", "size"),
        ("AdditionalFile", "sha1"),
        ("AdditionalFile", "sha256"),
        ("TargetReport", "abi"),
        ("TargetReport", "device"),
        ("TargetReport", "success"),
        ("TargetReport", "error"),
    ] {
        let pointer = format!("/{schema}/properties/{field}");
        assert!(
            schemas
                .and_then(|schemas| schemas.pointer(&pointer))
                .is_some(),
            "{schema}.{field}"
        );
    }
    let hash_description = schemas
        .and_then(|schemas| schemas.pointer("/DownloadInfo/properties/main_apk_sha1/description"))
        .and_then(Value::as_str);
    assert!(hash_description.is_some_and(|text| text.contains("base64url")));
    Ok(())
}
