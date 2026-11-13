use super::*;

#[test]
fn accepts_typical_package_names() {
    for name in [
        "com.discord",
        "com.google.android.apps.maps",
        "org.mozilla.firefox_beta",
        "a.b",
        "A1.b2_c3",
        "com.example.app123",
        "Com.Example.App",
    ] {
        assert!(is_valid_package_name(name), "{name}");
        assert!(validate_package_name(name).is_ok(), "{name}");
    }
}

#[test]
fn rejects_malformed_package_names() {
    for name in [
        "",
        "discord",
        ".",
        "com.",
        ".com",
        "com..discord",
        "com.discord.",
        "1com.discord",
        "com.1discord",
        "com._discord",
        "_com.discord",
        "com.dis-cord",
        "com.dis cord",
        "com/discord",
        "com.discord\n",
        " com.discord",
        "com.discord/../x",
        "com.discord?x=1",
        "com.d\u{eb}scord",
        "com.\u{661}abc",
        "\u{ff21}.b",
    ] {
        assert!(!is_valid_package_name(name), "{name:?}");
        let error = validate_package_name(name).err();
        assert_eq!(error.map(|e| e.status()), Some(400), "{name:?}");
    }
}

#[test]
fn enforces_the_length_limit() {
    let segment = "a".repeat(MAX_PACKAGE_NAME_LEN - 2);
    let longest = format!("{segment}.b");
    assert_eq!(longest.len(), MAX_PACKAGE_NAME_LEN);
    assert!(is_valid_package_name(&longest));
    let too_long = format!("a{longest}");
    assert!(!is_valid_package_name(&too_long));
}

#[test]
fn error_message_does_not_echo_the_input() {
    let error = validate_package_name("<script>alert(1)</script>").err();
    let message = error.map(|e| e.message().to_string()).unwrap_or_default();
    assert!(message.starts_with("Invalid package name"));
    assert!(!message.contains("script"));
}
