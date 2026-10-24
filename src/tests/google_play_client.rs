use super::*;
use googleplay_protobuf::{AppDetails, DocumentDetails, Item, TestingProgramInfo};

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
