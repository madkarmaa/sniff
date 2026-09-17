#[derive(Encode, Decode, Debug)]
struct EncodedDeviceProperties {
    pub device_configuration: Vec<u8>,
    pub android_checkin: Vec<u8>,
    pub extra_info: HashMap<String, String>,
}

#[derive(Debug)]
#[allow(dead_code)]
struct DeviceProperties {
    pub device_configuration: DeviceConfigurationProto,
    pub android_checkin: AndroidCheckinProto,
    pub extra_info: HashMap<String, String>,
}

#[allow(dead_code)]
impl EncodedDeviceProperties {
    #[must_use]
    pub const fn new(
        device_configuration: Vec<u8>,
        android_checkin: Vec<u8>,
        extra_info: HashMap<String, String>,
    ) -> Self {
        Self {
            device_configuration,
            android_checkin,
            extra_info,
        }
    }

    pub fn into_decoded(self) -> Result<DeviceProperties, prost::DecodeError> {
        Ok(DeviceProperties {
            device_configuration: DeviceConfigurationProto::decode(
                self.device_configuration.as_slice(),
            )?,
            android_checkin: AndroidCheckinProto::decode(self.android_checkin.as_slice())?,
            extra_info: self.extra_info,
        })
    }
}
