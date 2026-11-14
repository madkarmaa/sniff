use super::*;
use std::str::FromStr;

fn url(raw: &str) -> Url {
    Url::parse(raw).unwrap_or_else(|e| panic!("bad test URL {raw}: {e}"))
}

fn key(raw: &str, package_name: &str, channel: &str) -> Result<String, String> {
    Ok(cache_key(
        &url(raw),
        package_name,
        Channel::from_str(channel)?,
    ))
}

#[test]
fn cache_key_normalizes_channel_case() -> Result<(), String> {
    let lower = key(
        "https://example.test/v3/download/com.a.b/stable",
        "com.a.b",
        "stable",
    )?;
    let mixed = key(
        "https://example.test/v3/download/com.a.b/StAbLe",
        "com.a.b",
        "StAbLe",
    )?;
    assert_eq!(lower, mixed);
    assert_eq!(lower, "https://example.test/v3/download/com.a.b/stable");
    Ok(())
}

#[test]
fn cache_key_ignores_query_string_and_fragment() -> Result<(), String> {
    let plain = key(
        "https://example.test/v3/download/com.a.b/beta",
        "com.a.b",
        "beta",
    )?;
    let noisy = key(
        "https://example.test/v3/download/com.a.b/beta?utm=1&x=2#frag",
        "com.a.b",
        "beta",
    )?;
    assert_eq!(plain, noisy);
    Ok(())
}

#[test]
fn cache_key_separates_packages_channels_and_hosts() -> Result<(), String> {
    let base = key(
        "https://example.test/v3/download/com.a.b/stable",
        "com.a.b",
        "stable",
    )?;
    assert_ne!(
        base,
        key(
            "https://example.test/v3/download/com.a.c/stable",
            "com.a.c",
            "stable"
        )?
    );
    assert_ne!(
        base,
        key(
            "https://example.test/v3/download/com.a.b/beta",
            "com.a.b",
            "beta"
        )?
    );
    assert_ne!(
        base,
        key(
            "https://other.test/v3/download/com.a.b/stable",
            "com.a.b",
            "stable"
        )?
    );
    // Package names are case-sensitive on Google Play.
    assert_ne!(
        base,
        key(
            "https://example.test/v3/download/Com.a.b/stable",
            "Com.a.b",
            "stable"
        )?
    );
    Ok(())
}

#[test]
fn cache_key_is_built_from_inputs_not_the_request_path() -> Result<(), String> {
    // A trailing slash or double slash in the request must not fork the entry.
    let canonical = key(
        "https://example.test/v3/download/com.a.b/stable",
        "com.a.b",
        "stable",
    )?;
    let odd = key(
        "https://example.test/v3/download/com.a.b/stable/",
        "com.a.b",
        "stable",
    )?;
    assert_eq!(canonical, odd);
    assert!(canonical.contains(crate::API_PREFIX));
    Ok(())
}

#[test]
fn cache_control_matches_the_ttl() {
    assert_eq!(cache_control(TTL_SECS), "public, max-age=120");
}

#[test]
fn remaining_ttl_counts_down_with_age() {
    assert_eq!(remaining_ttl_secs(TTL_SECS, None), TTL_SECS);
    assert_eq!(remaining_ttl_secs(TTL_SECS, Some("0")), TTL_SECS);
    assert_eq!(remaining_ttl_secs(TTL_SECS, Some(" 20 ")), 100);
    assert_eq!(remaining_ttl_secs(TTL_SECS, Some("120")), 0);
    assert_eq!(remaining_ttl_secs(TTL_SECS, Some("9999")), 0);
    assert_eq!(remaining_ttl_secs(TTL_SECS, Some("garbage")), TTL_SECS);
    assert_eq!(remaining_ttl_secs(NOT_FOUND_TTL_SECS, Some("20")), 40);
}

#[test]
fn only_successes_and_unknown_apps_are_cacheable() {
    assert_eq!(ttl_secs(200), Some(TTL_SECS));
    assert_eq!(ttl_secs(404), Some(NOT_FOUND_TTL_SECS));
    assert!(NOT_FOUND_TTL_SECS < TTL_SECS);
    for status in [400, 422, 429, 500, 501, 502, 503] {
        assert_eq!(ttl_secs(status), None, "{status}");
    }
}
