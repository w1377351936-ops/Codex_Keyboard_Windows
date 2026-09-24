#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use easy_codex_host::dashscope::{DashScopeAsrClient, DashScopeTtsClient, TtsRequest};
    use easy_codex_host::paths::AppPaths;
    use easy_codex_host::secrets::{ImportLock, KeychainAccounts, SecretStore, WindowsDashScopeStore};

    let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?;
    let paths = AppPaths::from_root(std::path::Path::new(&local).join("EasyCodexInput"));
    paths.prepare()?;
    let lock = ImportLock::acquire(&paths.runtime_directory.join("key-import.lock"))?;
    let accounts = KeychainAccounts::load_or_create(&paths.installation_id, &lock)?;
    let store = WindowsDashScopeStore::new(&accounts);
    let audio = DashScopeTtsClient::default().synthesize(
        &store,
        &accounts,
        TtsRequest {
            text: "测试。",
            voice: "longanfengyue",
            instructions: "自然、清晰地读出这句话。",
        },
    )?;
    let receipt = audio.receipt();
    println!("status=ready");
    println!("model={}", receipt.model);
    println!("sample_rate={}", receipt.sample_rate);
    println!("samples={}", receipt.samples);
    println!("pcm_bytes={}", audio.pcm().len());
    let mut pcm16k = Vec::with_capacity(audio.pcm().len() / 3);
    for sample in audio.pcm().chunks_exact(6) {
        pcm16k.extend_from_slice(&sample[..2]);
    }
    let size = u32::try_from(pcm16k.len())?;
    let mut wav = Vec::with_capacity(44 + pcm16k.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(size + 36).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&16_000_u32.to_le_bytes());
    wav.extend_from_slice(&32_000_u32.to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&size.to_le_bytes());
    wav.extend_from_slice(&pcm16k);
    let secret = store.get(&accounts.dashscope)?.ok_or("DashScope credential missing")?;
    let transcript = DashScopeAsrClient::default().transcribe_wav(&secret, &wav)?;
    println!("asr_model={}", transcript.model);
    println!("asr_text={}", transcript.text);
    Ok(())
}

#[cfg(not(windows))]
fn main() {}
