use super::*;

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
