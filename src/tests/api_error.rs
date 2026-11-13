use super::*;
use crate::google_play_client::UpstreamError;
use std::io;

fn gpapi_error(kind: GpapiErrorKind) -> UpstreamError {
    Box::new(GpapiError::new(kind))
}

fn http(status: u16) -> UpstreamError {
    gpapi_error(GpapiErrorKind::Http {
        status,
        body: "secret upstream body".into(),
    })
}

fn classify(stage: Stage, error: &UpstreamError) -> ApiError {
    ApiError::from_upstream(stage, Channel::Beta, &**error)
}

#[test]
fn kinds_map_to_the_documented_status_codes() {
    let table = [
        (ErrorKind::InvalidRequest, 400),
        (ErrorKind::ChannelUnavailable, 400),
        (ErrorKind::NotFound, 404),
        (ErrorKind::NotDownloadable, 422),
        (ErrorKind::TooManyRequests, 429),
        (ErrorKind::Internal, 500),
        (ErrorKind::NotConfigured, 501),
        (ErrorKind::Upstream, 502),
        (ErrorKind::RateLimited, 503),
    ];
    for (kind, status) in table {
        assert_eq!(kind.status(), status, "{kind:?}");
        assert_eq!(ApiError::new(kind, "x").status(), status);
    }
}

#[test]
fn constructors_use_the_expected_kinds() {
    assert_eq!(ApiError::invalid_request("x").status(), 400);
    assert_eq!(ApiError::not_found("com.a.b").status(), 404);
    assert_eq!(
        ApiError::not_found("com.a.b").message(),
        "App 'com.a.b' not found"
    );
    let unavailable = ApiError::channel_unavailable(Channel::Alpha, "com.a.b");
    assert_eq!(unavailable.status(), 400);
    assert_eq!(
        unavailable.message(),
        "Channel 'alpha' is not available for package 'com.a.b'"
    );
    assert_eq!(ApiError::internal().status(), 500);
    assert_eq!(ApiError::too_many_requests().status(), 429);
}

#[test]
fn only_rate_limits_carry_retry_after() {
    assert_eq!(
        ApiError::rate_limited().retry_after_secs(),
        Some(RATE_LIMIT_RETRY_AFTER_SECS)
    );
    assert_eq!(
        ApiError::too_many_requests().retry_after_secs(),
        Some(crate::rate_limit::PERIOD_SECS)
    );
    for kind in [
        ErrorKind::Upstream,
        ErrorKind::Internal,
        ErrorKind::NotFound,
    ] {
        assert_eq!(ApiError::new(kind, "x").retry_after_secs(), None);
    }
}

#[test]
fn missing_credentials_depend_on_the_channel() {
    for channel in [Channel::Beta, Channel::Alpha] {
        let error = ApiError::credentials_missing(channel);
        assert_eq!(error.status(), 501);
        assert_eq!(
            error.message(),
            format!("The {channel} channel is not configured on this server")
        );
        // Environment variable names stay out of client messages.
        assert!(!error.message().contains("EMAIL"));
        assert!(!error.message().contains("TOKEN"));
    }
    let stable = ApiError::credentials_missing(Channel::Stable);
    assert_eq!(stable.status(), 500);
    assert!(!stable.message().to_lowercase().contains("stable"));
    assert_eq!(ApiError::misconfigured().status(), 500);
}

#[test]
fn upstream_429_is_a_503_with_retry_after() {
    for stage in [Stage::Login, Stage::Details, Stage::Download] {
        let error = classify(stage, &http(429));
        assert_eq!(error.kind(), ErrorKind::RateLimited);
        assert_eq!(error.status(), 503);
        assert!(error.retry_after_secs().is_some());
    }
}

#[test]
fn other_upstream_http_statuses_are_502_without_the_body() {
    for status in [400, 401, 403, 404, 500, 502, 503] {
        for stage in [Stage::Login, Stage::Details, Stage::Download] {
            let error = classify(stage, &http(status));
            assert_eq!(error.status(), 502, "{status} at {stage:?}");
            assert!(error.message().contains(&status.to_string()));
            assert!(!error.message().contains("secret"));
        }
    }
}

#[test]
fn invalid_app_is_unprocessable_only_for_downloads() {
    let error = classify(Stage::Download, &gpapi_error(GpapiErrorKind::InvalidApp));
    assert_eq!(error.kind(), ErrorKind::NotDownloadable);
    assert_eq!(error.status(), 422);
    for stage in [Stage::Login, Stage::Details] {
        let error = classify(stage, &gpapi_error(GpapiErrorKind::InvalidApp));
        assert_eq!(error.status(), 502, "{stage:?}");
    }
}

#[test]
fn login_failures_are_502_and_name_the_channel() {
    let kinds = [
        GpapiErrorKind::Authentication,
        GpapiErrorKind::TermsOfService,
        GpapiErrorKind::InvalidResponse,
        GpapiErrorKind::Str("No device config token".into()),
    ];
    for kind in kinds {
        let error = classify(Stage::Login, &gpapi_error(kind));
        assert_eq!(error.status(), 502);
        assert!(error.message().contains("beta"), "{}", error.message());
    }
}

#[test]
fn network_and_decode_failures_are_502() {
    let io: UpstreamError = Box::new(io::Error::other("connection reset by 10.0.0.1"));
    let wrapped = gpapi_error(GpapiErrorKind::IO(io::Error::other("boom")));
    let other = gpapi_error(GpapiErrorKind::Other("prost: buffer underflow".into()));
    for error in [io, wrapped, other] {
        for stage in [Stage::Details, Stage::Download] {
            let classified = classify(stage, &error);
            assert_eq!(classified.status(), 502);
            assert!(!classified.message().contains("10.0.0.1"));
            assert!(!classified.message().contains("underflow"));
        }
    }
}

#[test]
fn login_required_is_an_internal_error() {
    let error = classify(Stage::Details, &gpapi_error(GpapiErrorKind::LoginRequired));
    assert_eq!(error.status(), 500);
}

fn with_kind(kind: ErrorKind) -> ApiError {
    ApiError::new(kind, format!("{kind:?}"))
}

#[test]
fn aggregate_reports_unprocessable_only_when_every_target_agrees() {
    let all_422 = vec![with_kind(ErrorKind::NotDownloadable); 4];
    assert_eq!(ApiError::aggregate(&all_422).status(), 422);

    let mixed = vec![
        with_kind(ErrorKind::NotDownloadable),
        with_kind(ErrorKind::Upstream),
        with_kind(ErrorKind::NotDownloadable),
    ];
    assert_eq!(ApiError::aggregate(&mixed).status(), 502);
}

#[test]
fn aggregate_prefers_rate_limits_then_other_failures() {
    let errors = vec![
        with_kind(ErrorKind::Upstream),
        with_kind(ErrorKind::RateLimited),
        with_kind(ErrorKind::NotDownloadable),
    ];
    let aggregated = ApiError::aggregate(&errors);
    assert_eq!(aggregated.status(), 503);
    assert!(aggregated.retry_after_secs().is_some());

    let errors = vec![
        with_kind(ErrorKind::NotDownloadable),
        with_kind(ErrorKind::Internal),
    ];
    assert_eq!(ApiError::aggregate(&errors).status(), 500);
}

#[test]
fn aggregate_of_nothing_is_internal() {
    assert_eq!(ApiError::aggregate(&[]).status(), 500);
}
