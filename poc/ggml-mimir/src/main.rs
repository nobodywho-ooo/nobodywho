use anyhow::{bail, Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::time::Instant;
use tokenizers::Tokenizer;

mod model;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value = "models/DFM-Mimir")]
    model_dir: PathBuf,
    #[arg(long)]
    prompt: String,
    #[arg(long, default_value = "metal")]
    backend: String,
    #[arg(long, default_value_t = 4)]
    max_tokens: usize,
    #[arg(long, default_value_t = 8)]
    threads: usize,
    #[arg(long)]
    logits_output: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.prompt.trim().is_empty() || args.max_tokens == 0 {
        bail!("prompt and max-tokens must be non-empty");
    }
    let tokenizer = Tokenizer::from_file(args.model_dir.join("tokenizer.json"))
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let rendered = format!("<bos><|turn>user\n{}<turn|>\n<|turn>model\n", args.prompt);
    let encoding = tokenizer
        .encode(rendered, false)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let mut token_ids = encoding.get_ids().to_vec();
    let prefix_length = token_ids.len();
    println!("Encoded prompt into {prefix_length} tokens");

    let model = model::Model::load(&args.model_dir, &args.backend, args.threads)?;
    let available_tokens = model.max_context().saturating_sub(prefix_length);
    if available_tokens == 0 {
        bail!("prompt fills the model's context window");
    }
    let generation_limit = args.max_tokens.min(available_tokens);
    if generation_limit < args.max_tokens {
        println!("Limiting generation to {generation_limit} tokens to fit the context window");
    }
    let started = Instant::now();
    let mut generated = Vec::with_capacity(generation_limit);
    for index in 0..generation_limit {
        let token_started = Instant::now();
        let logits = model.logits(&token_ids, prefix_length)?;
        if let Some(path) = &args.logits_output {
            write_f32(path, &logits)?;
        }
        let token = logits
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map(|(token, _)| token as u32)
            .context("model returned no logits")?;
        token_ids.push(token);
        generated.push(token);
        let text = tokenizer
            .decode(&generated, true)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        println!(
            "Token {} in {:.2}s: {:?}",
            index + 1,
            token_started.elapsed().as_secs_f32(),
            text
        );
        if token == model.eos_token_id() {
            break;
        }
    }
    let text = tokenizer
        .decode(&generated, true)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!("\n{text}");
    println!(
        "Generated {} tokens in {:.2}s",
        generated.len(),
        started.elapsed().as_secs_f32()
    );
    Ok(())
}

fn write_f32(path: &PathBuf, values: &[f32]) -> Result<()> {
    let bytes = unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    };
    std::fs::write(path, bytes)?;
    Ok(())
}
