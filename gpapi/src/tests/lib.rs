use super::*;

#[test]
fn parse_form() {
    let form_reply = "FOO=BAR\nbaz=qux";
    let mut expected_reply = HashMap::new();
    expected_reply.insert("baz".to_string(), "qux".to_string());
    expected_reply.insert("foo".to_string(), "BAR".to_string());
    let parsed_form_reply = parse_form_reply(form_reply);
    assert_eq!(expected_reply, parsed_form_reply);
}

#[test]
fn form_post_encodes_values() {
    let params = HashMap::from([(String::from("Token"), String::from("oauth2_4/a+b=c&d"))]);
    assert_eq!(form_post(&params), "Token=oauth2_4%2Fa%2Bb%3Dc%26d");
}

#[test]
fn user_agent_contains_device() {
    let config = BuildConfiguration::new(
        "1.0",
        "100",
        "28",
        "device",
        "hardware",
        "product",
        "release",
        "model",
        "build",
        "arm64-v8a",
    );
    let agent = config.user_agent();
    assert!(agent.contains("device"));
    assert!(agent.contains("arm64-v8a"));
}

fn http_kind(status: u16) -> GpapiError {
    GpapiError::new(GpapiErrorKind::Http {
        status,
        body: String::new(),
    })
}

#[test]
fn http_status_is_exposed() {
    assert_eq!(http_kind(429).http_status(), Some(429));
    assert_eq!(
        GpapiError::new(GpapiErrorKind::InvalidApp).http_status(),
        None
    );
    assert_eq!(GpapiError::from("boom").http_status(), None);
}

#[test]
fn only_401_and_403_are_auth_failures() {
    assert!(http_kind(401).is_auth_failure());
    assert!(http_kind(403).is_auth_failure());
    for status in [400, 404, 429, 500, 503] {
        assert!(!http_kind(status).is_auth_failure(), "status {status}");
    }
    assert!(!GpapiError::new(GpapiErrorKind::Authentication).is_auth_failure());
}

#[test]
fn boxed_error_helpers_downcast() {
    let boxed: Box<dyn Error + Send + Sync> = Box::new(http_kind(401));
    assert!(crate::error::is_auth_failure(&*boxed));
    assert_eq!(crate::error::http_status(&*boxed), Some(401));
    let other: Box<dyn Error + Send + Sync> = "plain".into();
    assert!(!crate::error::is_auth_failure(&*other));
    assert_eq!(crate::error::http_status(&*other), None);
}

#[test]
fn http_error_display_includes_status_and_body() {
    assert_eq!(http_kind(500).to_string(), "Google Play returned HTTP 500");
    let err = http_error(403, b"  denied \n");
    assert_eq!(err.to_string(), "Google Play returned HTTP 403: denied");
}

#[test]
fn http_error_truncates_on_char_boundaries() {
    // Multi-byte chars make byte-based truncation land mid-character.
    let body = "é".repeat(MAX_ERROR_BODY_CHARS * 2);
    let err = http_error(502, body.as_bytes());
    let Some(GpapiErrorKind::Http { body: excerpt, .. }) =
        err.downcast_ref::<GpapiError>().map(GpapiError::kind)
    else {
        panic!("expected an HTTP error");
    };
    assert_eq!(excerpt.chars().count(), MAX_ERROR_BODY_CHARS);
}

#[test]
fn http_error_tolerates_invalid_utf8() {
    let err = http_error(500, &[0xff, 0xfe, b'x']);
    assert!(
        err.to_string()
            .starts_with("Google Play returned HTTP 500: ")
    );
}

#[test]
fn success_statuses() {
    assert!(is_success_status(200));
    assert!(is_success_status(204));
    assert!(!is_success_status(199));
    assert!(!is_success_status(301));
    assert!(!is_success_status(401));
}

#[test]
fn auth_reply_keeps_google_error_on_4xx() {
    let reply = parse_auth_reply(403, b"Error=BadAuthentication\nInfo=x\n").unwrap();
    assert_eq!(
        reply.get("error").map(String::as_str),
        Some("BadAuthentication")
    );
}

#[test]
fn auth_reply_parses_success_body() {
    let reply = parse_auth_reply(200, b"Auth=tok\nExpiry=0\n").unwrap();
    assert_eq!(reply.get("auth").map(String::as_str), Some("tok"));
}

#[test]
fn auth_reply_without_error_body_on_failure_is_http_error() {
    let err = parse_auth_reply(503, b"<html>unavailable</html>").unwrap_err();
    assert_eq!(crate::error::http_status(&*err), Some(503));
    let err = parse_auth_reply(429, &[0xff, 0xfe]).unwrap_err();
    assert_eq!(crate::error::http_status(&*err), Some(429));
}

#[test]
fn auth_reply_invalid_utf8_on_success_is_not_http_error() {
    let err = parse_auth_reply(200, &[0xff, 0xfe]).unwrap_err();
    assert_eq!(crate::error::http_status(&*err), None);
}

#[test]
fn new_accepts_known_device() {
    assert!(Gpapi::new("px_9a", "user@example.com").is_ok());
}

#[test]
fn new_is_repeatable_with_cached_device_database() {
    let first = Gpapi::new("px_9a", "a@example.com").unwrap();
    let second = Gpapi::new("px_9a", "b@example.com").unwrap();
    assert_eq!(
        first.device_properties.extra_info,
        second.device_properties.extra_info
    );
}

#[test]
fn new_rejects_unknown_device() {
    let Err(err) = Gpapi::new("no_such_device", "user@example.com") else {
        panic!("expected an error");
    };
    assert_eq!(err.to_string(), "invalid device codename: no_such_device");
}

#[test]
fn device_override_does_not_leak_into_shared_database() {
    let mut first = Gpapi::new("px_9a", "a@example.com").unwrap();
    first.set_supported_abis(["x86_64"]);
    let second = Gpapi::new("px_9a", "b@example.com").unwrap();
    assert_ne!(
        first.device_properties.device_configuration.native_platform,
        second
            .device_properties
            .device_configuration
            .native_platform
    );
}

#[test]
fn named_download_defaults_to_empty() {
    assert_eq!(NamedDownload::default().size, None);
    assert_eq!(DownloadInfo::default().main_apk_sha256, None);
}
