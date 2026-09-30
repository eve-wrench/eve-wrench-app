use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// Windows FILETIME: 100ns intervals since 1601-01-01, which is what EVE
// stamps on every settings value.
pub(crate) fn filetime_now() -> u64 {
    (unix_now() + 11_644_473_600) * 10_000_000
}

pub(crate) fn load_settings_json(path: &Path) -> Result<(serde_json::Value, bool), String> {
    let bytes = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    let decoded = blue_marshal::decode(&bytes)
        .map_err(|e| format!("Failed to decode settings file: {}", e))?;
    Ok((blue_marshal::to_json(&decoded.value), decoded.had_crc))
}

pub(crate) fn encode_settings_json(
    json: &serde_json::Value,
    checksum: bool,
) -> Result<Vec<u8>, String> {
    let value = blue_marshal::from_json(json)
        .map_err(|e| format!("Failed to rebuild settings data: {}", e))?;
    let options = blue_marshal::EncodeOptions {
        checksum,
        ..Default::default()
    };
    blue_marshal::encode(&value, &options)
        .map_err(|e| format!("Failed to encode settings file: {}", e))
}

// Strips blue-marshal's lossless type prefix ("utf8:lol" -> "lol").
pub(crate) fn strip_type_prefix(s: &str) -> &str {
    s.split_once(':').map(|(_, rest)| rest).unwrap_or(s)
}
