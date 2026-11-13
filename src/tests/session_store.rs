use super::*;

fn key(abis: &[&str]) -> ClientKey {
    (
        Channel::Beta,
        String::from("px_9a"),
        abis.iter().copied().map(String::from).collect(),
    )
}

fn state() -> SessionState {
    SessionState {
        // Beyond 2^53: must survive JSON read by JavaScript.
        gsf_id: 0x3a6f_1c2d_9e8b_7f61,
        device_checkin_consistency_token: Some(String::from("consistency")),
        device_config_token: String::from("config"),
        auth_token: Some(String::from("auth")),
        dfe_cookie: Some(String::from("cookie")),
    }
}

#[test]
fn keys_name_the_channel_device_and_abis() {
    assert_eq!(storage_key(&key(&[])), "session:v1:beta:px_9a:");
    assert_eq!(
        storage_key(&key(&["x86", "x86_64"])),
        "session:v1:beta:px_9a:x86,x86_64"
    );
}

#[test]
fn sessions_expire_after_the_ttl() {
    assert!(is_usable(1_000, 1_000, None));
    assert!(is_usable(1_000, 1_000 + SESSION_TTL_MS - 1, None));
    assert!(!is_usable(1_000, 1_000 + SESSION_TTL_MS, None));
    // A clock that went backwards does not make a session unusable.
    assert!(is_usable(5_000, 1_000, None));
}

#[test]
fn only_sessions_newer_than_a_rejected_one_are_usable() {
    assert!(!is_usable(1_000, 1_500, Some(1_000)));
    assert!(!is_usable(900, 1_500, Some(1_000)));
    assert!(is_usable(1_200, 1_500, Some(1_000)));
}

#[test]
fn stored_sessions_round_trip_through_json() -> Result<(), serde_json::Error> {
    let stored = StoredSession::new("user@example.test", state(), 42);
    let json = serde_json::to_string(&stored)?;
    assert!(json.contains(r#""gsf_id":"4210615158873751393""#));
    let read: StoredSession = serde_json::from_str(&json)?;
    assert_eq!(read, stored);
    assert_eq!(read.logged_in_at_ms, 42);
    assert_eq!(read.state_for("user@example.test"), Some(state()));
    Ok(())
}

#[test]
fn sessions_of_another_account_are_ignored() {
    let stored = StoredSession::new("old@example.test", state(), 42);
    assert_eq!(stored.state_for("new@example.test"), None);
}

#[test]
fn corrupt_device_ids_are_ignored() -> Result<(), serde_json::Error> {
    let mut json = serde_json::to_value(StoredSession::new("a@example.test", state(), 1))?;
    json["gsf_id"] = serde_json::Value::from("not a number");
    let stored: StoredSession = serde_json::from_value(json)?;
    assert_eq!(stored.state_for("a@example.test"), None);
    Ok(())
}
