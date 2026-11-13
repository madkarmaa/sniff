use crate::EncodedDeviceProperties;
use std::collections::HashMap;
use std::sync::LazyLock;

static DEVICES_ENCODED: &[u8] = include_bytes!("device_properties.bin");

/// The device database, decoded once per process. A decode failure is kept as
/// a message so `Gpapi::new` can report it as an error instead of panicking.
pub static DEVICES: LazyLock<Result<HashMap<String, EncodedDeviceProperties>, String>> =
    LazyLock::new(|| {
        bincode::borrow_decode_from_slice::<
            HashMap<String, EncodedDeviceProperties>,
            bincode::config::Configuration,
        >(DEVICES_ENCODED, bincode::config::standard())
        .map(|(devices, _)| devices)
        .map_err(|e| e.to_string())
    });
