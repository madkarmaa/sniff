use super::*;

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
