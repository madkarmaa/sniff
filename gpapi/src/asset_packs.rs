use crate::Gpapi;
use crate::error::{Error as GpapiError, ErrorKind as GpapiErrorKind};
use prost::Message;
use std::error::Error;

/// One APK of a Play Asset Delivery pack.
///
/// `sha256` is passed through exactly as Google sends it (base64url, not hex).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssetPackFile {
    pub pack_name: String,
    /// Identifies the APK within its pack; a pack can be split into several.
    pub slice_id: Option<String>,
    pub version_code: Option<i64>,
    pub download_url: Option<String>,
    /// Download size in bytes.
    pub size: Option<i64>,
    pub sha256: Option<String>,
}

/// Asks for plain APKs. The other formats split a pack into compressed chunks.
const COMPRESSION_FORMAT_UNSPECIFIED: u32 = 0;

#[derive(Clone, PartialEq, Message)]
struct AssetModuleDeliveryRequest {
    #[prost(string, optional, tag = "1")]
    package_name: Option<String>,
    #[prost(message, optional, tag = "2")]
    caller_info: Option<CallerInfo>,
    #[prost(uint32, repeated, packed = "false", tag = "4")]
    supported_compression_formats: Vec<u32>,
    #[prost(message, repeated, tag = "6")]
    modules: Vec<RequestedModule>,
}

#[derive(Clone, PartialEq, Message)]
struct CallerInfo {
    #[prost(int64, optional, tag = "1")]
    app_version_code: Option<i64>,
}

#[derive(Clone, PartialEq, Message)]
struct RequestedModule {
    #[prost(string, optional, tag = "1")]
    name: Option<String>,
}

// The response is a `ResponseWrapper` whose payload carries the answer in
// field 151, which googleplay-protobuf does not define.
#[derive(Clone, PartialEq, Message)]
struct ResponseWrapper {
    #[prost(message, optional, tag = "1")]
    payload: Option<Payload>,
}

#[derive(Clone, PartialEq, Message)]
struct Payload {
    #[prost(message, optional, tag = "151")]
    module_delivery_info: Option<ModuleDeliveryInfo>,
}

#[derive(Clone, PartialEq, Message)]
struct ModuleDeliveryInfo {
    #[prost(message, repeated, tag = "3")]
    modules: Vec<ModuleInfo>,
}

#[derive(Clone, PartialEq, Message)]
struct ModuleInfo {
    #[prost(string, optional, tag = "1")]
    name: Option<String>,
    #[prost(int64, optional, tag = "2")]
    version_code: Option<i64>,
    #[prost(message, repeated, tag = "3")]
    slices: Vec<SliceInfo>,
}

#[derive(Clone, PartialEq, Message)]
struct SliceInfo {
    #[prost(message, optional, tag = "1")]
    metadata: Option<SliceMetadata>,
    #[prost(message, optional, tag = "2")]
    full_download: Option<FullDownloadInfo>,
}

#[derive(Clone, PartialEq, Message)]
struct SliceMetadata {
    #[prost(string, optional, tag = "1")]
    slice_id: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
struct FullDownloadInfo {
    #[prost(message, repeated, tag = "4")]
    chunks: Vec<ChunkInfo>,
}

#[derive(Clone, PartialEq, Message)]
struct ChunkInfo {
    #[prost(int64, optional, tag = "1")]
    size: Option<i64>,
    #[prost(string, optional, tag = "2")]
    sha256: Option<String>,
    #[prost(string, optional, tag = "3")]
    download_url: Option<String>,
}

impl Gpapi {
    /// Retrieve the download URLs of the named asset packs of an app version.
    ///
    /// Only fast-follow and on-demand packs are served; Google Play delivers
    /// install-time packs as splits. Names Google Play does not know are
    /// left out of the result.
    ///
    /// # Errors
    ///
    /// Returns an error if login is required, if the request fails, or if a
    /// pack comes in a form other than one plain APK per slice.
    pub async fn get_asset_packs(
        &self,
        pkg_name: &str,
        version_code: i64,
        pack_names: &[String],
    ) -> Result<Vec<AssetPackFile>, Box<dyn Error + Send + Sync>> {
        if self.auth_token.is_none() {
            return Err(Box::new(GpapiError::new(GpapiErrorKind::LoginRequired)));
        }
        let request = AssetModuleDeliveryRequest {
            package_name: Some(pkg_name.to_string()),
            caller_info: Some(CallerInfo {
                app_version_code: Some(version_code),
            }),
            supported_compression_formats: vec![COMPRESSION_FORMAT_UNSPECIFIED],
            modules: pack_names
                .iter()
                .map(|name| RequestedModule {
                    name: Some(name.clone()),
                })
                .collect(),
        };
        let mut headers = self.get_default_headers()?;
        headers.insert("content-type", String::from("application/x-protobuf"));
        let bytes = self
            .execute_request_helper(
                "assetModuleDelivery",
                None,
                Some(&request.encode_to_vec()),
                headers,
                true,
            )
            .await?;
        asset_pack_files(&bytes)
    }
}

fn asset_pack_files(bytes: &[u8]) -> Result<Vec<AssetPackFile>, Box<dyn Error + Send + Sync>> {
    let modules = ResponseWrapper::decode(bytes)?
        .payload
        .and_then(|payload| payload.module_delivery_info)
        .map_or_default(|info| info.modules);
    let mut files = Vec::new();
    for module in modules {
        let pack_name = module.name.unwrap_or_default();
        for slice in module.slices {
            let chunks = slice
                .full_download
                .map_or_default(|download| download.chunks);
            // Plain APKs come whole; a split one would need its chunks joined.
            let [chunk] = <[ChunkInfo; 1]>::try_from(chunks).map_err(|chunks| {
                GpapiError::from(format!(
                    "asset pack {pack_name} has a slice in {} chunks",
                    chunks.len()
                ))
            })?;
            files.push(AssetPackFile {
                pack_name: pack_name.clone(),
                slice_id: slice.metadata.and_then(|metadata| metadata.slice_id),
                version_code: module.version_code,
                download_url: chunk.download_url,
                size: chunk.size,
                sha256: chunk.sha256,
            });
        }
    }
    Ok(files)
}

#[cfg(test)]
#[path = "tests/asset_packs.rs"]
mod tests;
