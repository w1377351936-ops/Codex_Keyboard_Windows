use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let paths = easy_codex_host::paths::AppPaths::from_root(
            std::path::Path::new(&local).join("EasyCodexInput"),
        );
        println!("host_secret_loaded={}", easy_codex_host::provisioning::load_device_secret(&paths).is_ok());
    }
    let api = hidapi::HidApi::new()?;
    let candidates: Vec<_> = api.device_list().filter(|d| {
        d.vendor_id() == 0x303a && d.product_id() == 0x1006
            && (d.interface_number() == 0 || d.interface_number() == -1)
    }).collect();
    println!("usb_candidates={}", candidates.len());
    let request_id = 0x7524_0913_u32;
    for candidate in candidates {
        let Ok(device) = candidate.open_device(&api) else { continue };
        let mut request = [0_u8; 17];
        request[0] = 0x13;
        request[1..4].copy_from_slice(b"S3R");
        request[4] = 1;
        request[5..9].copy_from_slice(&request_id.to_le_bytes());
        if device.send_feature_report(&request).is_err() { continue }
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut chunks: Vec<Option<Vec<u8>>> = Vec::new();
        let mut expected_len = 0_usize;
        let mut expected_crc = 0_u16;
        while Instant::now() < deadline {
            let mut report = [0_u8; 64];
            let Ok(len) = device.read_timeout(&mut report, 200) else { break };
            if len < 14 || report[0] != 0x11 || report[1] != 4 || report[5] != 1
                || u32::from_le_bytes(report[6..10].try_into()?) != request_id { continue }
            let index = report[2] as usize;
            let total = report[3] as usize;
            let data_len = report[4] as usize;
            if total == 0 || total > 11 || index >= total || data_len < 9 || data_len > 59
                || 5 + data_len > len { break }
            let wire_len = u16::from_le_bytes(report[10..12].try_into()?) as usize;
            let wire_crc = u16::from_le_bytes(report[12..14].try_into()?);
            if wire_len == 0 || wire_len > 512 { break }
            if chunks.is_empty() {
                chunks.resize(total, None);
                expected_len = wire_len;
                expected_crc = wire_crc;
            }
            if chunks.len() != total || expected_len != wire_len || expected_crc != wire_crc { break }
            chunks[index] = Some(report[14..5 + data_len].to_vec());
            if chunks.iter().all(Option::is_some) {
                let json: Vec<u8> = chunks.into_iter().flatten().flatten().collect();
                if json.len() != expected_len || crc16(&json) != expected_crc {
                    println!("status_integrity=failed");
                    return Ok(());
                }
                let value: serde_json::Value = serde_json::from_slice(&json)?;
                println!("status_integrity=ok");
                for key in ["phase", "status", "saved", "bytes", "crc16"] {
                    if let Some(field) = value.get(key) { println!("{key}={field}"); }
                }
                return Ok(());
            }
        }
    }
    println!("status_response=unavailable");
    Ok(())
}

fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0xffff_u16;
    for byte in bytes {
        crc ^= (*byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}
