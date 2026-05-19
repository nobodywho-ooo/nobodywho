use nobodywho::tts::{PiperConfig, TtsBuilder, TtsConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <model-dir> \"text\" [speaker_id]", args[0]);
        std::process::exit(1);
    }

    let mut cfg = PiperConfig::new(&args[1]);
    if let Some(id) = args.get(3).and_then(|s| s.parse().ok()) {
        cfg.speaker_id = id;
    }

    let tts = TtsBuilder::new(TtsConfig::Piper(cfg)).build()?;
    let wav = tts.synthesize(&args[2])?;
    play_wav(&wav);
    Ok(())
}

fn play_wav(wav: &[u8]) {
    let tmp = std::env::temp_dir().join("nobodywho_tts.wav");
    if std::fs::write(&tmp, wav).is_err() {
        eprintln!("Failed to write temp WAV");
        return;
    }
    for cmd in ["afplay", "paplay", "aplay"] {
        if std::process::Command::new(cmd)
            .arg(&tmp)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            let _ = std::fs::remove_file(&tmp);
            return;
        }
    }
    let _ = std::fs::remove_file(&tmp);
    eprintln!("No audio player found (tried afplay, paplay, aplay).");
}
