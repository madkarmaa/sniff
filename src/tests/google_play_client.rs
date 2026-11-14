use super::*;
use googleplay_protobuf::{AppDetails, Availability, DocumentDetails, Item, TestingProgramInfo};

fn details(subscribed: Option<bool>) -> DetailsResponse {
    DetailsResponse {
        item: Some(Item {
            details: Some(DocumentDetails {
                app_details: Some(AppDetails {
                    testing_program_info: subscribed.map(|subscribed| TestingProgramInfo {
                        subscribed: Some(subscribed),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[test]
fn testing_channels_require_testing_program_enrollment() {
    for channel in [Channel::Beta, Channel::Alpha] {
        assert!(channel.is_available_in(&details(Some(true))));
        assert!(!channel.is_available_in(&details(Some(false))));
        assert!(!channel.is_available_in(&details(None)));
    }
    assert!(Channel::Stable.is_available_in(&details(None)));
}

fn details_with_restriction(restriction: Option<i32>) -> DetailsResponse {
    DetailsResponse {
        item: Some(Item {
            availability: Some(Availability {
                restriction,
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[test]
fn availability_restriction_ignores_unrestricted_apps() {
    assert_eq!(
        availability_restriction(&details_with_restriction(Some(1))),
        None
    );
    assert_eq!(
        availability_restriction(&details_with_restriction(None)),
        None
    );
    assert_eq!(availability_restriction(&details(None)), None);
}

#[test]
fn availability_restriction_reports_restricted_apps() {
    assert_eq!(
        availability_restriction(&details_with_restriction(Some(31))),
        Some(31)
    );
}
