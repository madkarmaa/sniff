use super::*;

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
