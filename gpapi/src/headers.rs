use crate::Gpapi;
use crate::consts;
use crate::error::{Error as GpapiError, ErrorKind as GpapiErrorKind};
use googleplay_protobuf::AndroidBuildProto;
use std::collections::HashMap;
use std::error::Error;

impl Gpapi {
    pub(super) fn build(&self) -> Result<&AndroidBuildProto, Box<dyn Error + Send + Sync>> {
        self.device_properties
            .android_checkin
            .build
            .as_ref()
            .ok_or_else(invalid_response)
    }

    fn extra_info(&self, key: &str) -> Result<&str, Box<dyn Error + Send + Sync>> {
        self.device_properties
            .extra_info
            .get(key)
            .map(String::as_str)
            .ok_or_else(invalid_response)
    }

    pub(super) fn get_default_headers(
        &self,
    ) -> Result<HashMap<&str, String>, Box<dyn Error + Send + Sync>> {
        let mut headers = HashMap::new();
        self.append_default_headers(&mut headers)?;
        Ok(headers)
    }

    fn append_default_headers(
        &self,
        headers: &mut HashMap<&str, String>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Some(auth_token) = &self.auth_token {
            headers.insert("Authorization", format!("Bearer {auth_token}"));
        }

        let build = self.build()?;
        let build_configuration = BuildConfiguration::new(
            self.extra_info("Vending.versionString")?,
            self.extra_info("Vending.version")?,
            &build.sdk_version.ok_or_else(invalid_response)?.to_string(),
            build.device.as_deref().ok_or_else(invalid_response)?,
            build.product.as_deref().ok_or_else(invalid_response)?,
            build
                .build_product
                .as_deref()
                .ok_or_else(invalid_response)?,
            self.extra_info("Build.VERSION.RELEASE")?,
            build.model.as_deref().ok_or_else(invalid_response)?,
            self.extra_info("Build.ID")?,
            &self
                .device_properties
                .device_configuration
                .native_platform
                .join(";"),
        );

        headers.insert("user-agent", build_configuration.user_agent());

        if let Some(gsf_id) = &self.gsf_id {
            headers.insert("X-DFE-Device-Id", format!("{gsf_id:x}"));
        }
        headers.insert("accept-language", self.locale.replace('_', "-"));
        headers.insert(
            "X-DFE-Encoded-Targets",
            String::from(consts::defaults::DEFAULT_DFE_TARGETS),
        );
        headers.insert(
            "X-DFE-Phenotype",
            String::from(consts::defaults::DEFAULT_DFE_PHENOTYPE),
        );
        headers.insert("X-DFE-Client-Id", String::from("am-android-google"));
        headers.insert("X-DFE-Network-Type", String::from("4"));
        headers.insert("X-DFE-Content-Filters", String::new());
        headers.insert("X-Limit-Ad-Tracking-Enabled", String::from("false"));
        headers.insert("X-Ad-Id", String::new());
        headers.insert("X-DFE-UserLanguages", self.locale.clone());
        headers.insert("X-DFE-Request-Params", String::from("timeoutMs=4000"));
        if let Some(device_checkin_consistency_token) = &self.device_checkin_consistency_token {
            headers.insert(
                "X-DFE-Device-Checkin-Consistency-Token",
                device_checkin_consistency_token.clone(),
            );
        }
        if let Some(device_config_token) = &self.device_config_token {
            headers.insert("X-DFE-Device-Config-Token", device_config_token.clone());
        }
        if let Some(dfe_cookie) = &self.dfe_cookie {
            headers.insert("X-DFE-Cookie", dfe_cookie.clone());
        }
        if let Some(mcc_mcn) = self.device_properties.extra_info.get("SimOperator") {
            headers.insert("X-DFE-MCCMCN", mcc_mcn.clone());
        }
        Ok(())
    }

    pub(super) fn append_auth_headers(
        &self,
        headers: &mut HashMap<&str, String>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let build_device = self
            .build()?
            .device
            .as_deref()
            .ok_or_else(invalid_response)?;
        let build_id = self.extra_info("Build.ID")?;
        headers.insert(
            "app",
            String::from(consts::defaults::DEFAULT_ANDROID_VENDING),
        );
        headers.insert(
            "User-Agent",
            format!("GoogleAuth/1.4 ({build_device} {build_id})"),
        );
        if let Some(gsf_id) = self.gsf_id {
            headers.insert("device", format!("{gsf_id:x}"));
        }
        Ok(())
    }
}

pub fn invalid_response() -> Box<dyn Error + Send + Sync> {
    Box::new(GpapiError::new(GpapiErrorKind::InvalidResponse))
}

#[derive(Debug, Clone)]
struct BuildConfiguration {
    pub finsky_agent: String,
    pub finsky_version: String,
    pub api: String,
    pub version_code: String,
    pub sdk: String,
    pub device: String,
    pub hardware: String,
    pub product: String,
    pub platform_version_release: String,
    pub model: String,
    pub build_id: String,
    pub is_wide_screen: String,
    pub supported_abis: String,
}

impl BuildConfiguration {
    #[must_use]
    pub fn user_agent(&self) -> String {
        format!(
            "{}/{} (api={},versionCode={},sdk={},device={},hardware={},product={},platformVersionRelease={},model={},buildId={},isWideScreen={},supportedAbis={})",
            self.finsky_agent,
            self.finsky_version,
            self.api,
            self.version_code,
            self.sdk,
            self.device,
            self.hardware,
            self.product,
            self.platform_version_release,
            self.model,
            self.build_id,
            self.is_wide_screen,
            self.supported_abis,
        )
    }
}

impl BuildConfiguration {
    #[allow(clippy::too_many_arguments)]
    fn new(
        finsky_version: &str,
        version_code: &str,
        sdk: &str,
        device: &str,
        hardware: &str,
        product: &str,
        platform_version_release: &str,
        model: &str,
        build_id: &str,
        supported_abis: &str,
    ) -> Self {
        use consts::defaults::DEFAULT_FINSKY_AGENT;
        use consts::defaults::api_user_agent::{DEFAULT_API, DEFAULT_IS_WIDE_SCREEN};

        Self {
            finsky_agent: DEFAULT_FINSKY_AGENT.to_string(),
            finsky_version: finsky_version.to_string(),
            api: DEFAULT_API.to_string(),
            version_code: version_code.to_string(),
            sdk: sdk.to_string(),
            device: device.to_string(),
            hardware: hardware.to_string(),
            product: product.to_string(),
            platform_version_release: platform_version_release.to_string(),
            model: model.to_string(),
            build_id: build_id.to_string(),
            is_wide_screen: DEFAULT_IS_WIDE_SCREEN.to_string(),
            supported_abis: supported_abis.to_string(),
        }
    }
}

#[cfg(test)]
#[path = "tests/headers.rs"]
mod tests;
