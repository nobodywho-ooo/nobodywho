use anyhow::{bail, Result};
use clap::Parser;
use std::path::PathBuf;

mod supertonic;
mod text;
mod wav;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "models/supertonic-3")]
    model_dir: PathBuf,
    #[arg(long)]
    text: String,
    #[arg(long, default_value = "cpu")]
    backend: String,
    #[arg(long, default_value = "output.wav")]
    output: PathBuf,
    #[arg(long, default_value = "M1")]
    voice: String,
    #[arg(long, default_value = "en")]
    language: String,
    #[arg(long, default_value_t = 8)]
    steps: usize,
    #[arg(long, default_value_t = 1.05)]
    speed: f32,
    #[arg(long, default_value_t = 0)]
    seed: u64,
    #[arg(long, default_value_t = 4)]
    threads: usize,
    #[arg(long)]
    debug_dir: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.text.trim().is_empty() {
        bail!("text must not be empty");
    }
    let engine = supertonic::Engine::new(supertonic::EngineConfig {
        model_dir: args.model_dir,
        backend: args.backend,
        voice: args.voice,
        language: args.language.clone(),
        steps: args.steps,
        speed: args.speed,
        seed: args.seed,
        threads: args.threads,
        debug_dir: args.debug_dir,
    })?;
    let audio = engine.synthesize(text::preprocess(&args.text, &args.language))?;
    wav::write_mono_16(&args.output, &audio.samples, audio.sample_rate)?;
    Ok(())
}
