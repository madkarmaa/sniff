use super::*;
use serde_json::json;

#[test]
fn asset_packs_report_their_delivery_mode() -> Result<(), serde_json::Error> {
    let pack = |name: &str| gpapi::AssetPackFile {
        pack_name: name.into(),
        slice_id: Some(name.into()),
        version_code: Some(7),
        download_url: Some(format!("https://example.com/{name}.apk")),
        size: Some(2048),
        sha256: Some(format!("{name}-sha256")),
    };
    let files = asset_pack_files(vec![pack("levels"), pack("krisp")], &["levels".into()]);
    assert_eq!(
        serde_json::to_value(&files)?,
        json!([
            {
                "name": "levels",
                "slice_id": "levels",
                "delivery_mode": "fast_follow",
                "version_code": 7,
                "download_url": "https://example.com/levels.apk",
                "size": 2048,
                "sha256": "levels-sha256"
            },
            {
                "name": "krisp",
                "slice_id": "krisp",
                "delivery_mode": "on_demand",
                "version_code": 7,
                "download_url": "https://example.com/krisp.apk",
                "size": 2048,
                "sha256": "krisp-sha256"
            }
        ])
    );
    Ok(())
}
