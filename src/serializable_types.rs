use googleplay_protobuf::DetailsResponse;
use serde::Serialize;

mod details_response_serde {
    use googleplay_protobuf::{
        AppDetails, AppInfo, AppInfoContainer, AppInfoSection, DetailsResponse, DiscoveryBadge,
        DiscoveryBadgeLink, DocumentDetails, Feature, Features, Image, Item, Link, Offer,
        PlayerBadge,
    };
    use serde::ser::{SerializeStruct, Serializer};

    macro_rules! opt_field {
        ($state:ident, $name:literal, $opt:expr) => {
            if let Some(ref value) = $opt {
                $state.serialize_field($name, value)?;
            }
        };
    }

    macro_rules! opt_wrap {
        ($state:ident, $name:literal, $opt:expr, $wrap:ident) => {
            if let Some(ref value) = $opt {
                $state.serialize_field($name, &$wrap(value))?;
            }
        };
    }

    pub fn serialize<S>(details: &DetailsResponse, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("DetailsResponse", 10)?;

        opt_wrap!(state, "item", details.item, SerializableItem);

        opt_field!(state, "footer_html", details.footer_html);

        if !details.discovery_badge.is_empty() {
            let serializable_badges: Vec<SerializableDiscoveryBadge> = details
                .discovery_badge
                .iter()
                .map(SerializableDiscoveryBadge)
                .collect();
            state.serialize_field("discovery_badge", &serializable_badges)?;
        }

        opt_field!(state, "enable_reviews", details.enable_reviews);

        opt_wrap!(state, "features", details.features, SerializableFeatures);

        state.end()
    }

    struct SerializableItem<'a>(&'a Item);
    struct SerializableDiscoveryBadge<'a>(&'a DiscoveryBadge);
    struct SerializableFeatures<'a>(&'a Features);
    struct SerializableFeature<'a>(&'a Feature);
    struct SerializablePlayerBadge<'a>(&'a PlayerBadge);
    struct SerializableDiscoveryBadgeLink<'a>(&'a DiscoveryBadgeLink);
    struct SerializableImage<'a>(&'a Image);
    struct SerializableLink<'a>(&'a Link);
    struct SerializableAppInfo<'a>(&'a AppInfo);
    struct SerializableAppInfoSection<'a>(&'a AppInfoSection);
    struct SerializableAppInfoContainer<'a>(&'a AppInfoContainer);
    struct SerializableAppDetails<'a>(&'a AppDetails);
    struct SerializableDocumentDetails<'a>(&'a DocumentDetails);
    struct SerializableOffer<'a>(&'a Offer);

    impl serde::Serialize for SerializableItem<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let item = self.0;
            let mut state = serializer.serialize_struct("Item", 20)?;

            opt_field!(state, "id", item.id);

            opt_field!(state, "sub_id", item.sub_id);

            if let Some(ref value) = item.r#type {
                state.serialize_field("type", value)?;
            }

            opt_field!(state, "category_id", item.category_id);

            opt_field!(state, "title", item.title);

            opt_field!(state, "creator", item.creator);

            opt_field!(state, "description_html", item.description_html);

            if !item.offer.is_empty() {
                let serializable_offers: Vec<SerializableOffer> =
                    item.offer.iter().map(SerializableOffer).collect();
                state.serialize_field("offer", &serializable_offers)?;
            }

            opt_wrap!(state, "details", item.details, SerializableDocumentDetails);

            opt_field!(state, "subtitle", item.subtitle);

            opt_wrap!(state, "app_info", item.app_info, SerializableAppInfo);

            opt_field!(state, "mature", item.mature);

            opt_field!(
                state,
                "promotional_description",
                item.promotional_description
            );

            opt_field!(
                state,
                "available_for_preregistration",
                item.available_for_preregistration
            );

            opt_field!(state, "force_shareability", item.force_shareability);

            state.end()
        }
    }

    impl serde::Serialize for SerializableDiscoveryBadge<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let badge = self.0;
            let mut state = serializer.serialize_struct("DiscoveryBadge", 14)?;

            opt_field!(state, "label", badge.label);

            opt_wrap!(state, "image", badge.image, SerializableImage);

            opt_field!(state, "background_color", badge.background_color);

            opt_wrap!(
                state,
                "badge_container1",
                badge.badge_container1,
                SerializableDiscoveryBadgeLink
            );

            opt_field!(state, "is_plus_one", badge.is_plus_one);

            opt_field!(state, "aggregate_rating", badge.aggregate_rating);

            opt_field!(state, "user_star_rating", badge.user_star_rating);

            opt_field!(state, "download_count", badge.download_count);

            opt_field!(state, "download_units", badge.download_units);

            opt_field!(state, "content_description", badge.content_description);

            opt_wrap!(
                state,
                "player_badge",
                badge.player_badge,
                SerializablePlayerBadge
            );

            opt_field!(
                state,
                "family_age_range_badge",
                badge.family_age_range_badge
            );

            opt_field!(state, "family_category_badge", badge.family_category_badge);

            state.end()
        }
    }

    impl serde::Serialize for SerializableFeatures<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let features = self.0;
            let mut state = serializer.serialize_struct("Features", 2)?;

            if !features.feature_presence.is_empty() {
                let serializable_features: Vec<SerializableFeature> = features
                    .feature_presence
                    .iter()
                    .map(SerializableFeature)
                    .collect();
                state.serialize_field("feature_presence", &serializable_features)?;
            }

            if !features.feature_rating.is_empty() {
                let serializable_ratings: Vec<SerializableFeature> = features
                    .feature_rating
                    .iter()
                    .map(SerializableFeature)
                    .collect();
                state.serialize_field("feature_rating", &serializable_ratings)?;
            }

            state.end()
        }
    }

    impl serde::Serialize for SerializableFeature<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let feature = self.0;
            let mut state = serializer.serialize_struct("Feature", 2)?;

            opt_field!(state, "label", feature.label);

            opt_field!(state, "value", feature.value);

            state.end()
        }
    }

    impl serde::Serialize for SerializablePlayerBadge<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let badge = self.0;
            let mut state = serializer.serialize_struct("PlayerBadge", 1)?;

            opt_wrap!(state, "overlay_icon", badge.overlay_icon, SerializableImage);

            state.end()
        }
    }

    impl serde::Serialize for SerializableDiscoveryBadgeLink<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let link = self.0;
            let mut state = serializer.serialize_struct("DiscoveryBadgeLink", 3)?;

            opt_wrap!(state, "link", link.link, SerializableLink);

            state.end()
        }
    }

    impl serde::Serialize for SerializableImage<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let image = self.0;
            let mut state = serializer.serialize_struct("Image", 5)?;

            opt_field!(state, "image_url", image.image_url);

            state.end()
        }
    }

    impl serde::Serialize for SerializableLink<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let link = self.0;
            let mut state = serializer.serialize_struct("Link", 3)?;

            opt_field!(state, "uri", link.uri);

            state.end()
        }
    }

    impl serde::Serialize for SerializableAppInfo<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let app_info = self.0;
            let mut state = serializer.serialize_struct("AppInfo", 2)?;

            opt_field!(state, "title", app_info.title);

            if !app_info.section.is_empty() {
                let section: Vec<SerializableAppInfoSection> = app_info
                    .section
                    .iter()
                    .map(SerializableAppInfoSection)
                    .collect();

                state.serialize_field("section", &section)?;
            }

            state.end()
        }
    }

    impl serde::Serialize for SerializableAppInfoSection<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let section = self.0;
            let mut state = serializer.serialize_struct("AppInfoSection", 2)?;

            opt_field!(state, "label", section.label);

            opt_wrap!(
                state,
                "container",
                section.container,
                SerializableAppInfoContainer
            );

            state.end()
        }
    }

    impl serde::Serialize for SerializableAppInfoContainer<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let container = self.0;
            let mut state = serializer.serialize_struct("AppInfoContainer", 2)?;

            opt_wrap!(state, "image", container.image, SerializableImage);

            opt_field!(state, "description", container.description);

            state.end()
        }
    }

    impl serde::Serialize for SerializableAppDetails<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let details = self.0;
            let mut state = serializer.serialize_struct("AppDetails", 14)?;

            opt_field!(state, "developer_name", details.developer_name);

            opt_field!(state, "major_version_number", details.major_version_number);

            opt_field!(state, "version_code", details.version_code);

            opt_field!(state, "version_string", details.version_string);

            opt_field!(state, "title", details.title);

            opt_field!(state, "info_download_size", details.info_download_size);

            opt_field!(state, "developer_email", details.developer_email);

            opt_field!(state, "developer_website", details.developer_website);

            opt_field!(state, "info_download", details.info_download);

            opt_field!(state, "package_name", details.package_name);

            opt_field!(state, "recent_changes_html", details.recent_changes_html);

            opt_field!(state, "info_updated_on", details.info_updated_on);

            opt_field!(state, "app_type", details.app_type);

            opt_field!(state, "target_sdk_version", details.target_sdk_version);

            state.end()
        }
    }

    impl serde::Serialize for SerializableDocumentDetails<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let details = self.0;
            let mut state = serializer.serialize_struct("DocumentDetails", 1)?;

            opt_wrap!(
                state,
                "app_details",
                details.app_details,
                SerializableAppDetails
            );

            state.end()
        }
    }

    impl serde::Serialize for SerializableOffer<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let offer = self.0;
            let mut state = serializer.serialize_struct("Offer", 1)?;

            opt_field!(state, "micros", offer.micros);

            opt_field!(state, "currency_code", offer.currency_code);

            opt_field!(state, "formatted_amount", offer.formatted_amount);

            if !offer.converted_price.is_empty() {
                let converted_prices: Vec<SerializableOffer> = offer
                    .converted_price
                    .iter()
                    .map(SerializableOffer)
                    .collect();
                state.serialize_field("converted_price", &converted_prices)?;
            }

            opt_field!(
                state,
                "checkout_flow_required",
                offer.checkout_flow_required
            );

            opt_field!(state, "full_price_micros", offer.full_price_micros);

            opt_field!(state, "formatted_full_amount", offer.formatted_full_amount);

            opt_field!(state, "offer_type", offer.offer_type);

            opt_field!(state, "on_sale_date", offer.on_sale_date);

            if !offer.promotion_label.is_empty() {
                state.serialize_field("promotion_label", &offer.promotion_label)?;
            }

            opt_field!(state, "formatted_name", offer.formatted_name);

            opt_field!(state, "formatted_description", offer.formatted_description);

            opt_field!(state, "licensed_offer_type", offer.licensed_offer_type);

            opt_field!(state, "offer_id", offer.offer_id);

            opt_field!(state, "sale", offer.sale);

            opt_field!(
                state,
                "instant_purchase_enabled",
                offer.instant_purchase_enabled
            );

            opt_field!(state, "sale_message", offer.sale_message);

            state.end()
        }
    }
}

#[derive(Serialize)]
pub struct SerializableDetailsResponse(
    #[serde(with = "details_response_serde")] pub DetailsResponse,
);
