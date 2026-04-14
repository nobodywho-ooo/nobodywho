/// TTS demo — synthesizes speech using OuteTTS + WavTokenizer.
///
/// Usage:
///   cargo run --example tts_demo -- <outetts.gguf> <wavtokenizer.gguf> "text to speak"
///
/// Download models:
///   OuteTTS 0.2:  https://huggingface.co/OuteAI/OuteTTS-0.2-500M-GGUF
///   OuteTTS 0.3:  https://huggingface.co/OuteAI/OuteTTS-0.3-1B-GGUF
///   WavTokenizer: https://huggingface.co/novateur/WavTokenizer-large-speech-75token-GGUF
///                 (e.g. wavtokenizer-large-75-f16.gguf)
///
/// Output is saved to output.wav and played with `afplay` on macOS when available.
use nobodywho::llm;
use nobodywho::tts::{TextModelBackend, Tts, TtsBackendConfig, TtsRequest, TtsSpeakerProfile, VocoderBackend};
use std::sync::Arc;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let program_start = Instant::now();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    nobodywho::send_llamacpp_logs_to_tracing();

    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 && args.len() != 5 {
        eprintln!(
            "Usage: {} <outetts.gguf> <wavtokenizer.gguf> \"text to speak\" [speaker.json]",
            args[0]
        );
        std::process::exit(1);
    }

    let tts_model_path = &args[1];
    let vocoder_path = &args[2];
    let user_text = &args[3];
    let speaker_path = args.get(4);

    println!("Loading TTS model: {tts_model_path}");
    let tts_load_start = Instant::now();
    let tts_model = Arc::new(llm::get_model(tts_model_path, true, None)?);
    println!("Loaded TTS model in {:.2?}", tts_load_start.elapsed());

    println!("Loading vocoder: {vocoder_path}");
    let vocoder_load_start = Instant::now();
    let voc_model = Arc::new(llm::get_model(vocoder_path, true, None)?);
    println!("Loaded vocoder in {:.2?}", vocoder_load_start.elapsed());

    let backend = detect_backend(tts_model_path);
    let tts_init_start = Instant::now();
    let tts = Tts::new_with_backend(
        tts_model,
        voc_model,
        8192 * 2,
        TtsBackendConfig::new(backend.clone(), VocoderBackend::WavTokenizer75),
    );
    println!("Initialized TTS handles in {:.2?}", tts_init_start.elapsed());

    println!("Synthesizing: {user_text:?}");
    let synth_start = Instant::now();
    let request = match (backend, speaker_path) {
        (TextModelBackend::OuteTtsV03, Some(path)) => {
            let profile = TtsSpeakerProfile::from_path(path)?;
            TtsRequest::new(user_text).with_speaker_profile(profile)
        }
        _ => TtsRequest::new(user_text),
    };
    let wav_bytes = tts.synthesize_request(request)?;
    println!(
        "Synthesis completed in {:.2?} ({} bytes)",
        synth_start.elapsed(),
        wav_bytes.len()
    );

    let out_path = "output.wav";
    let write_start = Instant::now();
    std::fs::write(out_path, &wav_bytes)?;
    println!("Saved {out_path} in {:.2?}", write_start.elapsed());

    let playback_start = Instant::now();
    let status = std::process::Command::new("afplay").arg(out_path).status();
    match status {
        Ok(s) if s.success() => println!("Playback done in {:.2?}.", playback_start.elapsed()),
        Ok(_) => eprintln!("afplay exited with an error"),
        Err(e) => eprintln!("Could not run afplay: {e}  (open {out_path} manually)"),
    }

    println!("Total runtime: {:.2?}", program_start.elapsed());

    Ok(())
}

fn detect_backend(model_path: &str) -> TextModelBackend {
    if model_path.contains("0.3") {
        TextModelBackend::OuteTtsV03
    } else {
        TextModelBackend::OuteTtsV02
    }
}
