use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::Parser;
use serde_json::json;

mod engine;
mod weights;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "../models/gemma-4-E2B-it")]
    model_dir: PathBuf,
    #[arg(short = 'n', long, default_value_t = 32)]
    tokens: usize,
    #[arg(short = 'r', long, default_value_t = 5)]
    repetitions: usize,
    #[arg(long)]
    prompt_tokens: Option<PathBuf>,
    #[arg(long)]
    output: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.tokens == 0 || args.repetitions == 0 {
        bail!("token and repetition counts must be non-zero");
    }
    let stop_at_eos = args.prompt_tokens.is_some();
    let prompt = if let Some(path) = &args.prompt_tokens {
        serde_json::from_reader::<_, Vec<u32>>(std::fs::File::open(path)?)?
    } else {
        vec![2]
    };
    if prompt.is_empty() {
        bail!("prompt must contain at least one token");
    }
    let context = prompt.len() + args.tokens;
    let engine = engine::Engine::load(&args.model_dir, context)?;

    engine.clear_cache();
    let _ = engine.decode(prompt[0], 0)?;
    engine.clear_cache();

    let mut samples = Vec::with_capacity(args.repetitions);
    let mut final_tokens = Vec::new();
    for _ in 0..args.repetitions {
        engine.clear_cache();
        let started = Instant::now();
        let mut token = 0;
        for (position, prompt_token) in prompt.iter().enumerate() {
            token = engine.decode(*prompt_token, position)?;
        }
        let mut generated = Vec::with_capacity(args.tokens);
        generated.push(token);
        for index in 1..args.tokens {
            if stop_at_eos && matches!(token, 1 | 106) {
                break;
            }
            token = engine.decode(token, prompt.len() + index - 1)?;
            generated.push(token);
        }
        let elapsed = started.elapsed();
        samples.push(elapsed.as_secs_f64());
        final_tokens = generated;
    }
    let median_seconds = median(&samples)?;
    let report = json!({
        "tokens": final_tokens,
        "latency_ms": median_seconds * 1000.0,
        "tokens_per_second": final_tokens.len() as f64 / median_seconds,
        "repetitions": args.repetitions,
        "samples_ms": samples.iter().map(|seconds| seconds * 1000.0).collect::<Vec<_>>(),
        "runtime": "direct Metal quantized GEMV",
    });
    if let Some(path) = &args.output {
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn median(values: &[f64]) -> Result<f64> {
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    values
        .get(values.len() / 2)
        .copied()
        .context("no benchmark samples")
}
