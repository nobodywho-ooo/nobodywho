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
///
/// Output is saved to output.wav and played with `afplay` on macOS when available.
use nobodywho::tts::{Tts, TtsRequest};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let program_start = Instant::now();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let args: Vec<String> = std::env::args().collect();

    if args.get(1).map(|s| s.as_str()) == Some("--piper") {
        return run_piper(&args);
    }

    run_kokoro(&args)?;
    println!("Total runtime: {:.2?}", program_start.elapsed());
    play_output();
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

    std::fs::write("output.wav", &wav_bytes)?;
    println!("Saved output.wav");
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

    std::fs::write("output.wav", &wav_bytes)?;
    println!("Saved output.wav");

    play_output();
    Ok(())
}

fn play_output() {
    let status = std::process::Command::new("afplay")
        .arg("output.wav")
        .status();
    match status {
        Ok(s) if s.success() => println!("Playback done."),
        Ok(_) => eprintln!("afplay exited with an error"),
        Err(e) => eprintln!("Could not run afplay: {e}  (open output.wav manually)"),
    }
}
