use super::{ApiDoc, ApiResponse, DownloadInfo, DownloadResponse};
use crate::serializable_types::SerializableDetailsResponse;
use googleplay_protobuf::{AppDetails, DetailsResponse, DocumentDetails, Item};
use serde_json::{Value, json};
use utoipa::OpenApi;

#[test]
fn download_response_preserves_metadata_and_downloads() -> Result<(), serde_json::Error> {
    let details = DetailsResponse {
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
    };
    let metadata = serde_json::to_value(SerializableDetailsResponse(details.clone()))?;
    let download = DownloadInfo::from(gpapi::DownloadInfo {
        main_apk_url: Some("https://example.com/main.apk".into()),
        splits: vec![(
            Some("config.arm64_v8a".into()),
            Some("https://example.com/arm64.apk".into()),
        )],
        additional_files: vec![(
            Some("main.123.com.example.obb".into()),
            Some("https://example.com/main.obb".into()),
        )],
        dex_metadata_url: Some("https://example.com/main.dm".into()),
    });
    let urls = serde_json::to_value(&download)?;
    let response = serde_json::to_value(ApiResponse {
        success: true,
        data: Some(DownloadResponse {
            details: SerializableDetailsResponse(details),
            download,
        }),
        error: None,
    })?;
    let mut expected_data = metadata;
    if let (Some(data), Some(urls)) = (expected_data.as_object_mut(), urls.as_object()) {
        data.extend(urls.clone());
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
fn openapi_exposes_only_latest_v2_download() -> Result<(), serde_json::Error> {
    let spec = serde_json::to_value(ApiDoc::openapi())?;
    assert_eq!(spec.pointer("/info/version"), Some(&json!("2.0.0")));
    let paths = spec.get("paths").and_then(Value::as_object);
    assert_eq!(paths.map(serde_json::Map::len), Some(1));
    let operation = paths
        .and_then(|paths| paths.get("/v2/download/{package_name}/{channel}"))
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
