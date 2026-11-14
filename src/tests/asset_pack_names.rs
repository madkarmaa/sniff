use super::*;

fn names(raw: &str) -> Result<Vec<String>, ApiError> {
    requested_asset_packs(&url(raw))
}

fn url(raw: &str) -> Url {
    Url::parse(raw).unwrap_or_else(|e| panic!("bad test URL {raw}: {e}"))
}

#[test]
fn no_parameter_requests_no_packs() {
    assert_eq!(
        names("https://example.test/v3/download/com.a.b/stable").ok(),
        Some(vec![])
    );
}

#[test]
fn names_are_sorted_trimmed_and_deduplicated() {
    assert_eq!(
        names("https://example.test/x?asset_packs=krisp,%20voice_1,,krisp&asset_packs=assets").ok(),
        Some(vec!["assets".into(), "krisp".into(), "voice_1".into()])
    );
}

#[test]
fn malformed_names_are_rejected() {
    for query in [
        "a.b",
        "a-b",
        "a%2Fb",
        &"a".repeat(MAX_ASSET_PACK_NAME_LEN + 1),
    ] {
        let error = names(&format!("https://example.test/x?asset_packs={query}")).err();
        assert_eq!(error.map(|e| e.status()), Some(400), "{query}");
    }
}

#[test]
fn too_many_names_are_rejected() {
    let query = (0..=MAX_ASSET_PACK_NAMES)
        .map(|index| format!("pack{index}"))
        .collect::<Vec<_>>()
        .join(",");
    let error = names(&format!("https://example.test/x?asset_packs={query}")).err();
    assert_eq!(error.map(|e| e.status()), Some(400));
}
