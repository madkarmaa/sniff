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
