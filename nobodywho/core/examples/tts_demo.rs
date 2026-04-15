/// TTS demo — synthesizes speech using Kokoro (ONNX) or Piper (VITS + espeak-ng).
///
/// Usage:
///   Kokoro:
///     cargo run --example tts_demo -- <kokoro.onnx> <voices.bin> "text to speak" [voice] [speed] [language]
///
///   Piper:
///     cargo run --example tts_demo -- --piper <model.onnx> <model.onnx.json> "text to speak"
///
/// Download models:
///   Kokoro:
///     curl -LO https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/kokoro-v1.0.onnx
///     curl -LO https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/voices-v1.0.bin
///
///   Piper (Danish):
///     curl -LO https://huggingface.co/rhasspy/piper-voices/resolve/main/da/da_DK/talesyntese/medium/da_DK-talesyntese-medium.onnx
///     curl -LO https://huggingface.co/rhasspy/piper-voices/resolve/main/da/da_DK/talesyntese/medium/da_DK-talesyntese-medium.onnx.json
use nobodywho::tts::{Tts, TtsRequest};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let program_start = Instant::now();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let args: Vec<String> = std::env::args().collect();

    if args.get(1).map(|s| s.as_str()) == Some("--piper") {
        run_piper(&args)?;
    } else {
        run_kokoro(&args)?;
    }

    println!("Total runtime: {:.2?}", program_start.elapsed());
    Ok(())
}

fn run_kokoro(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 4 {
        eprintln!(
            "Usage: {} <kokoro.onnx> <voices.bin> \"text\" [voice] [speed] [language]",
            args[0]
        );
        eprintln!(
            "       {} --piper <model.onnx> <model.onnx.json> \"text\"",
            args[0]
        );
        eprintln!();
        eprintln!("Kokoro voices: af_heart, af_sarah, am_michael, bf_emma, bm_george, ...");
        eprintln!("Speed: 0.5 = slow, 1.0 = normal, 2.0 = fast");
        eprintln!("Language: en-us (default), en-gb, ja, zh, fr, hi, it, pt, es");
        std::process::exit(1);
    }

    let model_path = &args[1];
    let voices_path = &args[2];
    let text = &args[3];
    let voice = args.get(4).map(|s| s.as_str()).unwrap_or("af_heart");
    let speed: f32 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let language = args.get(6).map(|s| s.as_str()).unwrap_or("en-us");

    println!("Loading Kokoro model: {model_path}");
    let load_start = Instant::now();
    let tts = Tts::new(model_path, voices_path)?;
    println!("Loaded in {:.2?}", load_start.elapsed());

    let voices = tts.available_voices();
    println!("Available voices ({}):", voices.len());
    for v in &voices {
        print!("  {v}");
    }
    println!();

    println!("Synthesizing with voice={voice}, speed={speed}, language={language}: {text:?}");
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

fn run_piper(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 5 {
        eprintln!(
            "Usage: {} --piper <model.onnx> <model.onnx.json> \"text to speak\"",
            args[0]
        );
        std::process::exit(1);
    }

    let model_path = &args[2];
    let config_path = &args[3];
    let text = &args[4];

    println!("Loading Piper model: {model_path}");
    let load_start = Instant::now();
    let tts = Tts::new_piper(model_path, config_path)?;
    println!("Loaded in {:.2?}", load_start.elapsed());

    println!("Synthesizing: {text:?}");
    let synth_start = Instant::now();
    let wav_bytes = tts.synthesize(text.as_str())?;
    println!(
        "Synthesis completed in {:.2?} ({} bytes)",
        synth_start.elapsed(),
        wav_bytes.len()
    );

    play_wav(&wav_bytes);
    Ok(())
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
        let result = std::process::Command::new(cmd).arg(tmp_str.as_ref()).status();
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
