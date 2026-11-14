use super::*;

fn file(file_type: i32, asset_pack_name: Option<&str>) -> ListingFile {
    ListingFile {
        file_type: Some(file_type),
        asset_pack_name: asset_pack_name.map(String::from),
    }
}

fn listing_bytes(files: Vec<ListingFile>) -> Vec<u8> {
    ListingWrapper {
        payload: Some(ListingPayload {
            details_response: Some(ListingDetailsResponse {
                item: Some(ListingItem {
                    details: Some(ListingDocumentDetails {
                        app_details: Some(ListingAppDetails { files }),
                    }),
                }),
            }),
        }),
    }
    .encode_to_vec()
}

#[test]
fn only_asset_pack_files_name_fast_follow_packs() {
    let bytes = listing_bytes(vec![
        file(0, None),
        file(4, Some("cards_tutorial")),
        file(4, Some("localization")),
        file(0, Some("not_a_pack")),
    ]);
    assert_eq!(
        fast_follow_packs(&bytes).unwrap(),
        ["cards_tutorial", "localization"]
    );
}

#[test]
fn details_without_files_have_no_packs() {
    let bytes = ListingWrapper::default().encode_to_vec();
    assert_eq!(fast_follow_packs(&bytes).unwrap(), Vec::<String>::new());
}
