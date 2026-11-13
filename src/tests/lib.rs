use super::*;

#[test]
fn prefix_is_a_single_clean_path_segment() {
    assert!(API_PREFIX.starts_with("/v"));
    assert!(!API_PREFIX.ends_with('/'));
    assert!(
        API_PREFIX
            .trim_start_matches("/v")
            .chars()
            .all(|c| c.is_ascii_digit())
    );
}

#[test]
fn docs_page_points_at_this_versions_spec() {
    let html = scalar_html();
    assert!(html.contains(&format!("data-url=\"{API_PREFIX}/openapi.json\"")));
    assert!(!html.contains("/v2/"));
}

#[test]
fn crate_major_version_matches_the_prefix() {
    let major = env!("CARGO_PKG_VERSION").split('.').next();
    assert_eq!(
        major.map(|major| format!("/v{major}")).as_deref(),
        Some(API_PREFIX)
    );
}

#[test]
fn wrangler_config_serves_this_prefix() {
    let wrangler = include_str!("../../wrangler.toml");
    let host_pattern = format!("sniff.madkarma.top{API_PREFIX}");
    assert!(
        wrangler.contains(&format!("\"{host_pattern}\"")),
        "exact route"
    );
    assert!(
        wrangler.contains(&format!("\"{host_pattern}/*\"")),
        "wildcard route"
    );
    let name = format!("name = \"sniff-{}\"", API_PREFIX.trim_start_matches('/'));
    assert!(wrangler.contains(&name), "{name}");
}
