use super::*;

fn chunk(url: &str) -> ChunkInfo {
    ChunkInfo {
        size: Some(1_925_610),
        sha256: Some("jF6YTfJvvnBB5MrX57_WK0-fLrzuEhDq21UwmHJ8Lu4".into()),
        download_url: Some(url.into()),
    }
}

fn response(chunks: Vec<ChunkInfo>) -> Vec<u8> {
    ResponseWrapper {
        payload: Some(Payload {
            module_delivery_info: Some(ModuleDeliveryInfo {
                modules: vec![ModuleInfo {
                    name: Some("cards_tutorial".into()),
                    version_code: Some(203_110_007),
                    slices: vec![SliceInfo {
                        metadata: Some(SliceMetadata {
                            slice_id: Some("cards_tutorial".into()),
                        }),
                        full_download: Some(FullDownloadInfo { chunks }),
                    }],
                }],
            }),
        }),
    }
    .encode_to_vec()
}

#[test]
fn a_slice_becomes_one_file() {
    let files = asset_pack_files(&response(vec![chunk("https://example.com/pack")])).unwrap();
    assert_eq!(
        files,
        vec![AssetPackFile {
            pack_name: "cards_tutorial".into(),
            slice_id: Some("cards_tutorial".into()),
            version_code: Some(203_110_007),
            download_url: Some("https://example.com/pack".into()),
            size: Some(1_925_610),
            sha256: Some("jF6YTfJvvnBB5MrX57_WK0-fLrzuEhDq21UwmHJ8Lu4".into()),
        }]
    );
}

#[test]
fn unknown_packs_yield_no_files() {
    // Google Play answers a request for unknown names with only a status.
    let bytes = ResponseWrapper {
        payload: Some(Payload {
            module_delivery_info: Some(ModuleDeliveryInfo::default()),
        }),
    }
    .encode_to_vec();
    assert_eq!(asset_pack_files(&bytes).unwrap(), []);
}

#[test]
fn a_slice_in_several_chunks_is_an_error() {
    let bytes = response(vec![
        chunk("https://example.com/1"),
        chunk("https://example.com/2"),
    ]);
    assert!(asset_pack_files(&bytes).is_err());
}
