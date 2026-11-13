use crate::Gpapi;
use googleplay_protobuf::{
    AndroidCheckinRequest, AndroidCheckinResponse, UploadDeviceConfigRequest,
    UploadDeviceConfigResponse,
};
use prost::Message;
use std::collections::HashMap;
use std::error::Error;
use std::io::Cursor;

impl Gpapi {
    pub(super) async fn checkin(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let req = AndroidCheckinRequest {
            id: Some(0),
            checkin: Some(self.device_properties.android_checkin.clone()),
            locale: Some(self.locale.clone()),
            time_zone: Some(self.timezone.clone()),
            version: Some(3),
            device_configuration: Some(self.device_properties.device_configuration.clone()),
            fragment: Some(0),
            ..Default::default()
        };
        let mut bytes = Vec::with_capacity(req.encoded_len());
        req.encode(&mut bytes)?;

        let mut headers = HashMap::new();
        self.append_auth_headers(&mut headers)?;

        let resp = self.execute_checkin_request(&bytes, headers).await?;
        self.device_checkin_consistency_token = resp.device_checkin_consistency_token;
        self.gsf_id = resp.android_id.map(u64::cast_signed);
        Ok(())
    }

    async fn execute_checkin_request(
        &self,
        msg: &[u8],
        mut auth_headers: HashMap<&str, String>,
    ) -> Result<AndroidCheckinResponse, Box<dyn Error + Send + Sync>> {
        auth_headers.insert("content-type", String::from("application/x-protobuf"));
        let bytes = self
            .execute_request_helper("checkin", None, Some(msg), auth_headers, false)
            .await?;
        let resp = AndroidCheckinResponse::decode(&mut Cursor::new(bytes))?;
        Ok(resp)
    }

    pub(super) async fn upload_device_config(
        &self,
    ) -> Result<Option<UploadDeviceConfigResponse>, Box<dyn Error + Send + Sync>> {
        let req = UploadDeviceConfigRequest {
            device_configuration: Some(self.device_properties.device_configuration.clone()),
            ..Default::default()
        };
        let mut bytes = Vec::with_capacity(req.encoded_len());
        req.encode(&mut bytes)?;

        let mut headers = self.get_default_headers()?;
        headers.insert("content-type", String::from("application/x-protobuf"));

        let resp = self
            .execute_request("uploadDeviceConfig", None, Some(&bytes), headers)
            .await?;
        Ok(resp
            .payload
            .and_then(|payload| payload.upload_device_config_response))
    }
}
