// The `option_if_let_else` nursery lint fires on `serde`/`utoipa` derive
// expansions for `Option` fields; the reported spans point at the field
// definitions even though there is no `if let/else` in hand-written code.
#![allow(clippy::option_if_let_else)]

use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::handlers::get_download_info,
    ),
    components(
        schemas(
            ApiResponse<DownloadResponse<SerializableDetailsResponse>>,
            DownloadResponse<SerializableDetailsResponse>,
            ErrorResponse,
            SerializableDetailsResponse,
            DownloadInfo,
            SplitFile,
            AdditionalFile,
            Item,
            DocumentDetails,
            AppDetails,
            Offer,
            AppInfo,
            AppInfoSection,
            AppInfoContainer,
        )
    ),
    tags(
        (name = "Downloads", description = "Get latest app details and download information")
    ),
    info(
        title = "Sniff API",
        description = "API for retrieving the latest Google Play Store app details and download URLs across different release channels",
        version = env!("CARGO_PKG_VERSION"),
        contact(
            name = "MadKarma",
            url = "https://github.com/madkarmaa/sniff",
            email = "me@madkarma.top"
        )
    ),
    servers(
        (url = "/", description = "Current server")
    )
)]
pub struct ApiDoc;

#[derive(Serialize, Deserialize, ToSchema)]
#[allow(clippy::option_if_let_else)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

/// App metadata and download URLs share the same response object.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct DownloadResponse<T> {
    #[serde(flatten)]
    pub details: T,
    #[serde(flatten)]
    pub download: DownloadInfo,
}

/// Error envelope. Matches the runtime error shape exactly: `data` is
/// always `null` here, only `error` carries the message.
#[derive(Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "success": false,
    "data": null,
    "error": "App 'com.example' not found"
}))]
pub struct ErrorResponse {
    pub success: bool,
    pub data: Option<String>,
    pub error: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "item": {
        "id": "com.discord",
        "sub_id": "com.discord",
        "type": 1,
        "category_id": 3,
        "title": "Discord - Talk, Play, Hang Out",
        "creator": "Discord Inc.",
        "description_html": "Discord is designed for gaming and great for just chilling with friends or building a community. Customize your own space and gather your friends to talk while playing your favorite games, or just hang out.<br><br>GROUP CHAT THAT'S ALL FUN & GAMES<br>∙ Discord is great for playing games and chilling with friends, or even building a worldwide community. Customize your own space to talk, play, and hang out in.",
        "promotional_description": "Group Chat That's Fun & Games",
        "mature": false,
        "available_for_preregistration": false,
        "force_shareability": false,
        "offer": [{
            "micros": 0,
            "currency_code": "EUR",
            "formatted_amount": "",
            "checkout_flow_required": false,
            "offer_type": 1
        }],
        "details": {
            "app_details": {
                "developer_name": "Discord Inc.",
                "version_code": 289_020,
                "version_string": "289.20 - Stable",
                "info_download_size": 180_070_862,
                "developer_email": "support@discord.com",
                "developer_website": "https://dis.gd/contact",
                "info_download": "500,000,000+ downloads",
                "package_name": "com.discord",
                "recent_changes_html": "We've been hard at work making Discord better for you. This includes bug fixes and performance enhancements.",
                "info_updated_on": "Jul 21, 2025",
                "target_sdk_version": 35
            }
        },
        "app_info": {
            "section": [
                {
                    "label": "In-app purchases",
                    "container": {
                        "description": "€1.99 - €274.99 if billed through Play"
                    }
                },
                {
                    "label": "Offered by",
                    "container": {
                        "description": "Google Commerce Ltd"
                    }
                },
                {
                    "label": "Released on",
                    "container": {
                        "description": "May 13, 2015"
                    }
                }
            ]
        }
    },
    "footer_html": "All prices include VAT.",
    "enable_reviews": true
}))]
pub struct SerializableDetailsResponse {
    pub item: Option<Item>,
    pub footer_html: Option<String>,
    pub enable_reviews: Option<bool>,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "main_apk_url": "https://play.googleapis.com/download/by-token/download?token=AOTCm0Q...",
    "splits": [
        {
            "name": "config.arm64_v8a",
            "download_url": "https://play.googleapis.com/download/by-token/download?token=AOTCm0R..."
        },
        {
            "name": "config.en",
            "download_url": "https://play.googleapis.com/download/by-token/download?token=AOTCm0S..."
        }
    ],
    "additional_files": []
}))]
pub struct DownloadInfo {
    #[schema(example = "https://play.googleapis.com/download/by-token/download?token=AOTCm0Q...")]
    pub main_apk_url: Option<String>,
    pub splits: Vec<SplitFile>,
    pub additional_files: Vec<AdditionalFile>,
    pub dex_metadata_url: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct SplitFile {
    #[schema(example = "config.arm64_v8a")]
    pub name: Option<String>,
    #[schema(example = "https://play.googleapis.com/download/by-token/download?token=AOTCm0R...")]
    pub download_url: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct AdditionalFile {
    #[schema(example = "main.1234.com.example.obb")]
    pub filename: Option<String>,
    #[schema(example = "https://play.googleapis.com/download/by-token/download?token=AOTCm0T...")]
    pub download_url: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct Item {
    #[schema(example = "com.discord")]
    pub id: Option<String>,
    #[schema(example = "com.discord")]
    pub sub_id: Option<String>,
    #[schema(example = 1)]
    pub r#type: Option<i32>,
    #[schema(example = 3)]
    pub category_id: Option<i32>,
    #[schema(example = "Discord - Talk, Play, Hang Out")]
    pub title: Option<String>,
    #[schema(example = "Discord Inc.")]
    pub creator: Option<String>,
    pub description_html: Option<String>,
    #[schema(example = "Group Chat That's Fun & Games")]
    pub promotional_description: Option<String>,
    #[schema(example = false)]
    pub mature: Option<bool>,
    #[schema(example = false)]
    pub available_for_preregistration: Option<bool>,
    #[schema(example = false)]
    pub force_shareability: Option<bool>,
    pub offer: Vec<Offer>,
    pub details: Option<DocumentDetails>,
    pub app_info: Option<AppInfo>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct DocumentDetails {
    pub app_details: Option<AppDetails>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct AppDetails {
    #[schema(example = "Discord Inc.")]
    pub developer_name: Option<String>,
    #[schema(example = 289_020)]
    pub version_code: Option<i64>,
    #[schema(example = "289.20 - Stable")]
    pub version_string: Option<String>,
    #[schema(example = 180_070_862)]
    pub info_download_size: Option<i64>,
    #[schema(example = "support@discord.com")]
    pub developer_email: Option<String>,
    #[schema(example = "https://dis.gd/contact")]
    pub developer_website: Option<String>,
    #[schema(example = "500,000,000+ downloads")]
    pub info_download: Option<String>,
    #[schema(example = "com.discord")]
    pub package_name: Option<String>,
    pub recent_changes_html: Option<String>,
    #[schema(example = "Jul 21, 2025")]
    pub info_updated_on: Option<String>,
    #[schema(example = 35)]
    pub target_sdk_version: Option<i32>,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[allow(clippy::struct_field_names)]
pub struct Offer {
    #[schema(example = 0)]
    pub micros: Option<i64>,
    #[schema(example = "EUR")]
    pub currency_code: Option<String>,
    #[schema(example = "")]
    pub formatted_amount: Option<String>,
    #[schema(example = false)]
    pub checkout_flow_required: Option<bool>,
    // Field name matches the upstream Play API / protobuf JSON name.
    #[schema(example = 1)]
    pub offer_type: Option<i32>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct AppInfo {
    pub section: Vec<AppInfoSection>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct AppInfoSection {
    #[schema(example = "In-app purchases")]
    pub label: Option<String>,
    pub container: Option<AppInfoContainer>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct AppInfoContainer {
    #[schema(example = "€1.99 - €274.99 if billed through Play")]
    pub description: Option<String>,
}

impl From<gpapi::DownloadInfo> for DownloadInfo {
    fn from(gpapi_download_info: gpapi::DownloadInfo) -> Self {
        let (main_apk_url, splits_data, additional_files_data, dex_metadata_url) =
            gpapi_download_info;

        let splits = splits_data
            .into_iter()
            .map(|(name, url)| SplitFile {
                name,
                download_url: url,
            })
            .collect();

        let additional_files = additional_files_data
            .into_iter()
            .map(|(filename, url)| AdditionalFile {
                filename,
                download_url: url,
            })
            .collect();

        Self {
            main_apk_url,
            splits,
            additional_files,
            dex_metadata_url,
        }
    }
}

#[cfg(test)]
// Tests use assertions for contract failures and `?` for serialization errors.
#[allow(clippy::panic_in_result_fn)]
mod tests {
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
        let download = DownloadInfo::from((
            Some("https://example.com/main.apk".into()),
            vec![(
                Some("config.arm64_v8a".into()),
                Some("https://example.com/arm64.apk".into()),
            )],
            vec![(
                Some("main.123.com.example.obb".into()),
                Some("https://example.com/main.obb".into()),
            )],
            Some("https://example.com/main.dm".into()),
        ));
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
}
