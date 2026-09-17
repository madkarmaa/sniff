use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use configparser::ini::Ini;
use prost::Message;

use googleplay_protobuf::{
    AndroidBuildProto, AndroidCheckinProto, DeviceConfigurationProto, DeviceFeature,
};

use bincode::{Decode, Encode};
include!("src/device_properties.rs");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo::rerun-if-changed=device.properties");
    println!("cargo::rerun-if-changed=build.rs");
    if Path::new("src/device_properties.bin").exists() {
        return Ok(());
    }
    generate_device_properties_bin()
}

fn generate_device_properties_bin() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config()?;
    let mut device_properties_map = HashMap::new();
    for section in config.sections() {
        let (key, value) = process_section(&config, &section)?;
        device_properties_map.insert(key, value);
    }

    let devices_encoded: Vec<u8> =
        bincode::encode_to_vec(&device_properties_map, bincode::config::standard())?;

    let mut file = File::create("src/device_properties.bin")?;
    file.write_all(&devices_encoded)?;
    Ok(())
}

fn load_config() -> Result<Ini, Box<dyn std::error::Error>> {
    let mut config = Ini::new();
    let contents = fs::read_to_string("device.properties")?;
    config
        .read(contents)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(config)
}

fn process_section(
    config: &Ini,
    section: &str,
) -> Result<(String, EncodedDeviceProperties), Box<dyn std::error::Error>> {
    let extra_info = build_extra_info(config, section);
    let android_checkin = build_android_checkin(config, section)?;
    let device_configuration = build_device_configuration(config, section)?;

    let mut android_checkin_encoded = Vec::with_capacity(android_checkin.encoded_len());
    android_checkin.encode(&mut android_checkin_encoded)?;

    let mut device_configuration_encoded = Vec::with_capacity(device_configuration.encoded_len());
    device_configuration.encode(&mut device_configuration_encoded)?;

    let encoded = EncodedDeviceProperties::new(
        device_configuration_encoded,
        android_checkin_encoded,
        extra_info,
    );
    let key = section.replace("gplayapi_", "").replace(".properties", "");
    Ok((key, encoded))
}

#[must_use]
fn build_extra_info(config: &Ini, section: &str) -> HashMap<String, String> {
    let mut extra_info: HashMap<String, String> = [
        "Build.ID",
        "Vending.versionString",
        "Vending.version",
        "Build.VERSION.RELEASE",
    ]
    .into_iter()
    .map(|key| {
        (
            String::from(key),
            config.get(section, key).unwrap_or_default(),
        )
    })
    .collect();
    if let Some(sim_operator) = config.get(section, "SimOperator") {
        extra_info.insert("SimOperator".to_string(), sim_operator);
    }
    extra_info
}

fn build_android_checkin(
    config: &Ini,
    section: &str,
) -> Result<AndroidCheckinProto, Box<dyn std::error::Error>> {
    let android_build = AndroidBuildProto {
        id: config.get(section, "Build.FINGERPRINT"),
        product: config.get(section, "Build.HARDWARE"),
        carrier: config.get(section, "Build.BRAND"),
        radio: config.get(section, "Build.RADIO"),
        bootloader: config.get(section, "Build.BOOTLOADER"),
        device: config.get(section, "Build.DEVICE"),
        sdk_version: get_int_as_i32(config, section, "Build.VERSION.SDK_INT")?,
        model: config.get(section, "Build.MODEL"),
        manufacturer: config.get(section, "Build.MANUFACTURER"),
        build_product: config.get(section, "Build.PRODUCT"),
        client: config.get(section, "Client"),
        ota_installed: Some(false),
        google_services: get_int_as_i32(config, section, "GSF.version")?,
        ..Default::default()
    };
    Ok(AndroidCheckinProto {
        build: Some(android_build),
        last_checkin_msec: Some(0),
        cell_operator: config.get(section, "CellOperator"),
        sim_operator: config.get(section, "SimOperator"),
        roaming: config.get(section, "Roaming"),
        user_number: Some(0),
        ..Default::default()
    })
}

fn build_device_configuration(
    config: &Ini,
    section: &str,
) -> Result<DeviceConfigurationProto, Box<dyn std::error::Error>> {
    let system_shared_library = get_required_list(config, section, "SharedLibraries")?;
    let system_available_feature = get_required_list(config, section, "Features")?;
    let native_platform: Vec<String> = config
        .get(section, "Platforms")
        .unwrap_or_default()
        .split(',')
        .map(String::from)
        .collect();
    let system_supported_locale = get_required_list(config, section, "Locales")?;
    let gl_extension = get_required_list(config, section, "GL.Extensions")?;
    let features_raw = get_required_string(config, section, "Features")?;
    let device_feature: Vec<DeviceFeature> = features_raw
        .split(',')
        .map(|s| DeviceFeature {
            name: Some(String::from(s)),
            value: Some(0),
        })
        .collect();

    Ok(DeviceConfigurationProto {
        touch_screen: get_int_as_i32(config, section, "TouchScreen")?,
        keyboard: get_int_as_i32(config, section, "Keyboard")?,
        navigation: get_int_as_i32(config, section, "Navigation")?,
        screen_layout: get_int_as_i32(config, section, "ScreenLayout")?,
        has_hard_keyboard: get_bool(config, section, "HasHardKeyboard")?,
        has_five_way_navigation: get_bool(config, section, "HasFiveWayNavigation")?,
        screen_density: get_int_as_i32(config, section, "Screen.Density")?,
        gl_es_version: get_int_as_i32(config, section, "GL.Version")?,
        system_shared_library,
        system_available_feature,
        native_platform,
        screen_width: get_int_as_i32(config, section, "Screen.Width")?,
        screen_height: get_int_as_i32(config, section, "Screen.Height")?,
        system_supported_locale,
        gl_extension,
        device_feature,
        ..Default::default()
    })
}

fn get_int_as_i32(
    config: &Ini,
    section: &str,
    key: &str,
) -> Result<Option<i32>, Box<dyn std::error::Error>> {
    let opt_i64 = config
        .getint(section, key)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    opt_i64
        .map(i32::try_from)
        .transpose()
        .map_err(|e| -> Box<dyn std::error::Error> {
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })
}

fn get_bool(
    config: &Ini,
    section: &str,
    key: &str,
) -> Result<Option<bool>, Box<dyn std::error::Error>> {
    config
        .getbool(section, key)
        .map_err(|e| -> Box<dyn std::error::Error> {
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })
}

fn get_required_string(
    config: &Ini,
    section: &str,
    key: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    config
        .get(section, key)
        .ok_or_else(|| -> Box<dyn std::error::Error> {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("missing required key '{key}' in section '{section}'"),
            ))
        })
}

fn get_required_list(
    config: &Ini,
    section: &str,
    key: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let raw = get_required_string(config, section, key)?;
    Ok(raw.split(',').map(String::from).collect())
}
