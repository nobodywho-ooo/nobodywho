/// TTS demo — synthesizes speech using Kokoro, Piper, or Chatterbox.
///
/// The backend is auto-detected from the arguments:
///   .bin  → Kokoro (voices file)
///   .json → Piper (config file)
///   --chatterbox <dir> → Chatterbox (model directory)
///
/// Usage:
///   Kokoro:
///     cargo run --example tts_demo -- <kokoro.onnx> <voices.bin> "text" [voice] [speed] [language]
///
///   Piper:
///     cargo run --example tts_demo -- <model.onnx> <model.onnx.json> "text"
///
///   Chatterbox:
///     cargo run --example tts_demo -- --chatterbox <model_dir> "text" [language] [voice.wav] [exaggeration]
///
/// Download models:
///   Kokoro:
///     curl -LO https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/kokoro-v1.0.onnx
///     curl -LO https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/voices-v1.0.bin
///
///   Piper (Danish):
///     curl -LO https://huggingface.co/rhasspy/piper-voices/resolve/main/da/da_DK/talesyntese/medium/da_DK-talesyntese-medium.onnx
///     curl -LO https://huggingface.co/rhasspy/piper-voices/resolve/main/da/da_DK/talesyntese/medium/da_DK-talesyntese-medium.onnx.json
///
///   Chatterbox Multilingual:
///     See https://huggingface.co/onnx-community/chatterbox-multilingual-ONNX
use nobodywho::tts::{Tts, TtsDevice, TtsRequest};
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

    if args[1] == "--roest" {
        run_roest(&args)?;
    } else if args[1] == "--chatterbox" {
        run_chatterbox(&args)?;
    } else {
        run_kokoro_or_piper(&args)?;
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
    let voice = args.get(4).map(|s| s.as_str()).unwrap_or("af_heart");
    let speed: f32 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let language = if end > 6 { args[6].as_str() } else { "en-us" };

    println!("Loading model: {model_path}");
    let load_start = Instant::now();
    let tts = Tts::new(model_path, second_path, device)?;
    println!("Loaded in {:.2?}", load_start.elapsed());

    let voices = tts.available_voices();
    if !voices.is_empty() {
        println!("Available voices ({}):", voices.len());
        for v in &voices {
            print!("  {v}");
        }
        println!();
    }

    println!("Synthesizing: {text:?}");
    let synth_start = Instant::now();
    let request = TtsRequest::new(text.as_str())
        .with_voice(voice)
        .with_speed(speed)
        .with_language(language);
    let wav_bytes = tts.synthesize_request(request)?;
    println!(
        "Synthesis completed in {:.2?} ({} bytes)",
        synth_start.elapsed(),
        wav_bytes.len()
    );

    play_wav(&wav_bytes);
    Ok(())
}

fn run_roest(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    // --roest <dir> "text" [language] [device]
    if args.len() < 4 {
        eprintln!("Usage: {} --roest <model_dir> \"text\" [language]", args[0]);
        std::process::exit(1);
    }

    let (device, end) = parse_trailing_device(args)?;
    let model_dir = &args[2];
    let text = &args[3];
    let language = if end > 4 { args[4].as_str() } else { "" };

    println!("Loading Røst from: {model_dir}");
    let load_start = Instant::now();
    let tts = Tts::new_roest(model_dir, device)?;
    println!("Loaded in {:.2?}", load_start.elapsed());

    println!("Synthesizing: {text:?}");
    let synth_start = Instant::now();
    let request = TtsRequest::new(text.as_str()).with_language(language);
    let wav_bytes = tts.synthesize_request(request)?;
    println!(
        "Synthesis completed in {:.2?} ({} bytes)",
        synth_start.elapsed(),
        wav_bytes.len()
    );

    play_wav(&wav_bytes);
    Ok(())
}

fn run_chatterbox(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    // --chatterbox <dir> "text" [language] [voice.wav] [temperature] [top_k] [top_p] [device]
    if args.len() < 4 {
        print_usage(&args[0]);
        std::process::exit(1);
    }

    let (device, end) = parse_trailing_device(args)?;
    let model_dir = &args[2];
    let text = &args[3];
    let language = if end > 4 { args[4].as_str() } else { "" };
    let voice_wav = args.get(5).filter(|_| end > 5).and_then(|s| {
        if s.ends_with(".wav") {
            Some(std::path::PathBuf::from(s))
        } else {
            None
        }
    });
    // Sampling args shift by 1 if voice.wav is provided
    let sampling_offset = if voice_wav.is_some() { 6 } else { 5 };
    let temperature: f32 = if end > sampling_offset {
        args[sampling_offset].parse().unwrap_or(0.0)
    } else {
        0.0
    };
    let top_k: usize = if end > sampling_offset + 1 {
        args[sampling_offset + 1].parse().unwrap_or(0)
    } else {
        0
    };
    let top_p: f32 = if end > sampling_offset + 2 {
        args[sampling_offset + 2].parse().unwrap_or(1.0)
    } else {
        1.0
    };

    println!("Loading Chatterbox from: {model_dir}");
    let load_start = Instant::now();
    let tts = Tts::new_chatterbox(model_dir, voice_wav.as_deref(), device)?;
    println!("Loaded in {:.2?}", load_start.elapsed());

    println!(
        "Synthesizing ({language}): {text:?}  (temp={temperature}, top_k={top_k}, top_p={top_p})"
    );
    let synth_start = Instant::now();
    let request = TtsRequest::new(text.as_str())
        .with_language(language)
        .with_temperature(temperature)
        .with_top_k(top_k)
        .with_top_p(top_p);
    let wav_bytes = tts.synthesize_request(request)?;
    println!(
        "Synthesis completed in {:.2?} ({} bytes)",
        synth_start.elapsed(),
        wav_bytes.len()
    );

    play_wav(&wav_bytes);
    Ok(())
}

fn print_usage(program: &str) {
    eprintln!("Usage:");
    eprintln!("  Kokoro/Piper: {program} <model.onnx> <voices.bin|config.json> \"text\" [voice] [speed] [language] [device]");
    eprintln!("  Røst:         {program} --roest <model_dir> \"text\" [language] [device]");
    eprintln!("  Chatterbox:   {program} --chatterbox <model_dir> \"text\" [language] [voice.wav] [temperature] [top_k] [top_p] [device]");
    eprintln!();
    eprintln!("Devices: auto (default), cpu, cuda");
    eprintln!("Chatterbox languages: ar, da, de, el, en, es, fi, fr, he, hi, it, ja, ko, ms, nl, no, pl, pt, ru, sv, sw, tr, zh");
}

fn parse_device(s: &str) -> Result<TtsDevice, Box<dyn std::error::Error>> {
    match s.to_ascii_lowercase().as_str() {
        "auto" => Ok(TtsDevice::Auto),
        "cpu" => Ok(TtsDevice::Cpu),
        "cuda" => Ok(TtsDevice::Cuda),
        _ => Err(format!("invalid device `{s}`; expected auto, cpu, or cuda").into()),
    }
}

fn parse_trailing_device(
    args: &[String],
) -> Result<(TtsDevice, usize), Box<dyn std::error::Error>> {
    if let Some(last) = args.last() {
        match last.to_ascii_lowercase().as_str() {
            "auto" | "cpu" | "cuda" => return Ok((parse_device(last)?, args.len() - 1)),
            _ => {}
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
    let players = ["afplay", "paplay", "aplay"];

    for cmd in players {
        let result = std::process::Command::new(cmd)
            .arg(tmp_str.as_ref())
            .status();
        match result {
            Ok(s) if s.success() => {
                println!("Playback done ({cmd}).");
                let _ = std::fs::remove_file(&tmp);
                return;
            }
            _ => continue,
        }
    }

    let _ = std::fs::remove_file(&tmp);
    eprintln!("No audio player found. Install afplay (macOS), paplay, or aplay (Linux).");
}
