//! Read the saved EasyInput V2 config fingerprint over USB HID.
//! The response JSON is kept in memory and never printed.

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::{Duration, Instant};

    const VID: u16 = 0x303A;
    const PID: u16 = 0x1006;
    const REQUEST_ID: u32 = 0x3a4b5c6d;
    const MAX_JSON: usize = 2048;

    let api = hidapi::HidApi::new()?;
    let mut opened = 0;
    for candidate in api.device_list().filter(|device| {
        device.vendor_id() == VID
            && device.product_id() == PID
            && matches!(device.interface_number(), 0 | -1)
    }) {
        let Ok(device) = candidate.open_device(&api) else {
            continue;
        };
        opened += 1;
        let mut request = [0_u8; 17];
        request[0] = 0x13;
        request[1..4].copy_from_slice(b"S3R");
        request[4] = 1;
        request[5..9].copy_from_slice(&REQUEST_ID.to_le_bytes());
        if device.send_feature_report(&request).is_err() {
            continue;
        }

        let deadline = Instant::now() + Duration::from_secs(4);
        let mut chunks: Vec<Option<Vec<u8>>> = Vec::new();
        let mut expected_len = None;
        let mut expected_crc = None;
        while Instant::now() < deadline {
            let mut report = [0_u8; 64];
            let Ok(len) = device.read_timeout(&mut report, 200) else {
                break;
            };
            if len < 14 || report[0] != 0x11 || report[1] != 0x04 || report[5] != 1 {
                continue;
            }
            if u32::from_le_bytes(report[6..10].try_into()?) != REQUEST_ID {
                continue;
            }
            let index = report[2] as usize;
            let total = report[3] as usize;
            let payload_len = report[4] as usize;
            let json_len = u16::from_le_bytes(report[10..12].try_into()?) as usize;
            let crc = u16::from_le_bytes(report[12..14].try_into()?);
            if total == 0
                || total > 48
                || index >= total
                || payload_len < 9
                || payload_len > 59
                || json_len == 0
                || json_len > MAX_JSON
                || 14 + payload_len - 9 > len
            {
                continue;
            }
            if chunks.is_empty() {
                chunks.resize(total, None);
                expected_len = Some(json_len);
                expected_crc = Some(crc);
            }
            if chunks.len() != total || expected_len != Some(json_len) || expected_crc != Some(crc) {
                continue;
            }
            chunks[index] = Some(report[14..14 + payload_len - 9].to_vec());
            if chunks.iter().all(Option::is_some) {
                let bytes: Vec<u8> = chunks.into_iter().flatten().flatten().collect();
                if bytes.len() != json_len || crc16_ccitt(&bytes) != crc {
                    break;
                }
                let status: serde_json::Value = serde_json::from_slice(&bytes)?;
                println!("status=received");
                println!("phase={}", status["phase"].as_str().unwrap_or("unknown"));
                println!("saved={}", status["saved"].as_bool().unwrap_or(false));
                println!("config_bytes={}", status["bytes"].as_u64().unwrap_or(0));
                println!("config_crc16={}", status["crc16"].as_u64().unwrap_or(0));
                // These fields contain bounded device state, not the provisioned
                // SSID, Wi-Fi password, device key, or raw status payload.
                for field in ["capture", "control_state", "last_error"] {
                    if let Some(value) = status["audio"][field].as_str() {
                        println!("audio_{field}={value}");
                    }
                }
                return Ok(());
            }
        }
    }
    Err(format!("no valid USB status response; opened_interfaces={opened}").into())
}

#[cfg(windows)]
fn crc16_ccitt(bytes: &[u8]) -> u16 {
    let mut crc = 0xffff_u16;
    for byte in bytes {
        crc ^= (*byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This diagnostic is Windows-only");
}
