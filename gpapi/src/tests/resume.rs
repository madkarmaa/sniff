use super::*;

fn state() -> SessionState {
    SessionState {
        gsf_id: i64::MIN + 7,
        device_checkin_consistency_token: Some(String::from("consistency")),
        device_config_token: String::from("config"),
        auth_token: Some(String::from("auth")),
        dfe_cookie: None,
    }
}

#[test]
fn new_client_has_no_session_to_persist() {
    let api = Gpapi::new("px_9a", "user@example.com").unwrap();
    assert!(!api.has_registered_device());
    assert_eq!(api.session_state(), None);
}

#[test]
fn restored_session_round_trips() {
    let mut api = Gpapi::new("px_9a", "user@example.com").unwrap();
    api.restore_session(state());
    assert!(api.has_registered_device());
    assert_eq!(api.session_state(), Some(state()));
}

#[test]
fn restored_session_drives_the_request_headers() {
    let mut api = Gpapi::new("px_9a", "user@example.com").unwrap();
    api.restore_session(state());
    let headers = api.get_default_headers().unwrap();
    assert_eq!(
        headers.get("X-DFE-Device-Id").map(String::as_str),
        Some("8000000000000007")
    );
    assert_eq!(
        headers.get("Authorization").map(String::as_str),
        Some("Bearer auth")
    );
    assert_eq!(
        headers.get("X-DFE-Device-Config-Token").map(String::as_str),
        Some("config")
    );
    assert!(!headers.contains_key("X-DFE-Cookie"));
}
