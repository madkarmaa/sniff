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
fn aas_request_sends_droidguard_placeholder() {
    let req = AuthRequest::new("user@example.com", "oauth2_4/abc");
    assert_eq!(
        req.params.get("droidguard_results").map(String::as_str),
        Some("null")
    );
    assert_eq!(req.params.get("add_account").map(String::as_str), Some("1"));
    assert_eq!(req.params.get("service").map(String::as_str), Some("ac2dm"));
}

#[test]
fn aas_token_is_taken_from_reply() {
    let reply = parse_form_reply("Token=aas_et/xyz\nEmail=user@example.com\n");
    assert_eq!(aas_token_from_reply(reply).unwrap(), "aas_et/xyz");
}

#[test]
fn aas_reply_without_token_reports_googles_reason() {
    let reply = parse_auth_reply(400, b"Error=MissingDroidguard\n").unwrap();
    let err = aas_token_from_reply(reply).unwrap_err();
    assert_eq!(err.to_string(), "authentication failed: MissingDroidguard");
}

#[test]
fn aas_reply_without_token_or_reason_is_still_explained() {
    let err = aas_token_from_reply(HashMap::new()).unwrap_err();
    assert_eq!(err.to_string(), "authentication failed: no token in reply");
    let err = aas_token_from_reply(parse_form_reply("Error=\n")).unwrap_err();
    assert_eq!(err.to_string(), "authentication failed: no token in reply");
}
