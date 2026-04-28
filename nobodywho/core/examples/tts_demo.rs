/// TTS demo — synthesizes speech using Kokoro, Piper, Chatterbox, or Røst.
///
/// Usage:
///   Kokoro:     cargo run --example tts_demo -- <kokoro.onnx> <voices.bin> "text" [voice] [speed] [language] [device]
///   Piper:      cargo run --example tts_demo -- <model.onnx> <model.onnx.json> "text" [device]
///   Chatterbox: cargo run --example tts_demo -- --chatterbox <dir> "text" [language] [voice.wav] [device]
///   Røst:       cargo run --example tts_demo -- --roest <dir> "text" [language] [device]
///
/// Devices: auto (default), cpu, cuda
use nobodywho::tts::{
    ChatterboxConfig, KokoroConfig, PiperConfig, RoestConfig, Tts, TtsBuilder, TtsConfig, TtsDevice,
};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let program_start = Instant::now();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        print_usage(&args[0]);
        std::process::exit(1);
    }

    match args[1].as_str() {
        "--roest" => run_roest(&args)?,
        "--chatterbox" => run_chatterbox(&args)?,
        _ => run_kokoro_or_piper(&args)?,
    }

    println!("Total runtime: {:.2?}", program_start.elapsed());
    Ok(())
}

fn run_kokoro_or_piper(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 4 {
        print_usage(&args[0]);
        std::process::exit(1);
    }

    let (device, end) = parse_trailing_device(args)?;
    let model_path = &args[1];
    let second_path = &args[2];
    let text = &args[3];

    let config = if second_path.ends_with(".json") {
        TtsConfig::Piper(PiperConfig::new(model_path, second_path))
    } else {
        let mut cfg = KokoroConfig::new(model_path, second_path);
        if let Some(v) = args.get(4).filter(|_| end > 4) {
            cfg.voice = v.clone();
        }
        if let Some(s) = args.get(5).filter(|_| end > 5).and_then(|s| s.parse().ok()) {
            cfg.speed = s;
        }
        if let Some(l) = args.get(6).filter(|_| end > 6) {
            cfg.language = l.clone();
        }
        TtsConfig::Kokoro(cfg)
    };

    println!("Loading model: {model_path}");
    let load_start = Instant::now();
    let tts = TtsBuilder::new(config).with_device(device).build()?;
    println!("Loaded in {:.2?}", load_start.elapsed());

    list_voices(&tts);
    synthesize_and_play(&tts, text)
}

fn run_roest(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 4 {
        eprintln!(
            "Usage: {} --roest <model_dir> \"text\" [language] [device]",
            args[0]
        );
        std::process::exit(1);
    }

    let (device, end) = parse_trailing_device(args)?;
    let model_dir = &args[2];
    let text = &args[3];

    let mut config = RoestConfig::new(model_dir);
    if let Some(l) = args.get(4).filter(|_| end > 4) {
        config.language = l.clone();
    }

    println!("Loading Røst from: {model_dir}");
    let load_start = Instant::now();
    let tts = TtsBuilder::new(TtsConfig::Roest(config))
        .with_device(device)
        .build()?;
    println!("Loaded in {:.2?}", load_start.elapsed());

    synthesize_and_play(&tts, text)
}

fn run_chatterbox(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 4 {
        print_usage(&args[0]);
        std::process::exit(1);
    }

    let (device, end) = parse_trailing_device(args)?;
    let model_dir = &args[2];
    let text = &args[3];

    let mut config = ChatterboxConfig::new(model_dir);
    if let Some(l) = args.get(4).filter(|_| end > 4) {
        config.language = l.clone();
    }
    if let Some(wav) = args.get(5).filter(|s| end > 5 && s.ends_with(".wav")) {
        config.reference_wav = Some(wav.into());
    }

    println!("Loading Chatterbox from: {model_dir}");
    let load_start = Instant::now();
    let tts = TtsBuilder::new(TtsConfig::Chatterbox(config))
        .with_device(device)
        .build()?;
    println!("Loaded in {:.2?}", load_start.elapsed());

    synthesize_and_play(&tts, text)
}

fn synthesize_and_play(tts: &Tts, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("Synthesizing: {text:?}");
    let synth_start = Instant::now();
    let wav_bytes = tts.synthesize(text)?;
    println!(
        "Synthesis completed in {:.2?} ({} bytes)",
        synth_start.elapsed(),
        wav_bytes.len()
    );

    play_wav(&wav_bytes);
    Ok(())
}

fn list_voices(tts: &Tts) {
    let voices = tts.available_voices();
    if !voices.is_empty() {
        println!("Available voices ({}):", voices.len());
        for v in &voices {
            print!("  {v}");
        }
        println!();
    }
}

fn print_usage(program: &str) {
    eprintln!("Usage:");
    eprintln!("  Kokoro/Piper: {program} <model.onnx> <voices.bin|config.json> \"text\" [voice] [speed] [language] [device]");
    eprintln!("  Røst:         {program} --roest <model_dir> \"text\" [language] [device]");
    eprintln!("  Chatterbox:   {program} --chatterbox <model_dir> \"text\" [language] [voice.wav] [device]");
    eprintln!();
    eprintln!("Devices: auto (default), cpu, cuda");
}

fn parse_trailing_device(
    args: &[String],
) -> Result<(TtsDevice, usize), Box<dyn std::error::Error>> {
    if let Some(last) = args.last() {
        let device = match last.to_ascii_lowercase().as_str() {
            "auto" => Some(TtsDevice::Auto),
            "cpu" => Some(TtsDevice::Cpu),
            "cuda" => Some(TtsDevice::Cuda),
            _ => None,
        };
        if let Some(d) = device {
            return Ok((d, args.len() - 1));
        }
    }
    Ok((TtsDevice::Auto, args.len()))
}

fn play_wav(wav_bytes: &[u8]) {
    let tmp = std::env::temp_dir().join("nobodywho_tts_demo.wav");
    if std::fs::write(&tmp, wav_bytes).is_err() {
        eprintln!("Failed to write temp WAV file");
        return;
    }

    let tmp_str = tmp.to_string_lossy();
    // afplay: macOS, paplay: PulseAudio (Linux), aplay: ALSA (Linux)
    for cmd in ["afplay", "paplay", "aplay"] {
        if let Ok(s) = std::process::Command::new(cmd)
            .arg(tmp_str.as_ref())
            .status()
        {
            if s.success() {
                println!("Playback done ({cmd}).");
                let _ = std::fs::remove_file(&tmp);
                return;
            }
        }
    }

    let _ = std::fs::remove_file(&tmp);
    eprintln!("No audio player found. Install afplay (macOS), paplay, or aplay (Linux).");
}
