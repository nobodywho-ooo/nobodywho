use anyhow::{bail, Context, Result};
use clap::Parser;
use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, Instant};

mod model;

const GREEDY_SEED_TOKEN: i32 = 2;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value = "models/gemma-4-E2B-it")]
    model_dir: PathBuf,
    #[arg(short = 'p', long, default_value_t = 512)]
    prompt_tokens: usize,
    #[arg(short = 'n', long, default_value_t = 128)]
    generation_tokens: usize,
    #[arg(short = 'r', long, default_value_t = 5)]
    repetitions: usize,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    flash_attention: bool,
    #[arg(long)]
    greedy_tokens_output: Option<PathBuf>,
    #[arg(long)]
    greedy_report_output: Option<PathBuf>,
    #[arg(long)]
    greedy_prompt_tokens: Option<PathBuf>,
    #[arg(long)]
    greedy_only: bool,
    #[arg(long, default_value_t = 8)]
    greedy_tokens: usize,
}

struct BenchmarkResult {
    name: String,
    tokens: usize,
    samples: Vec<Duration>,
}

impl BenchmarkResult {
    fn average_tokens_per_second(&self) -> f64 {
        self.tokens as f64 * self.samples.len() as f64
            / self.samples.iter().map(Duration::as_secs_f64).sum::<f64>()
    }

    fn standard_deviation(&self) -> f64 {
        standard_deviation(&self.tokens_per_second())
    }

    fn tokens_per_second(&self) -> Vec<f64> {
        self.samples
            .iter()
            .map(|duration| self.tokens as f64 / duration.as_secs_f64())
            .collect()
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.prompt_tokens == 0 || args.generation_tokens == 0 || args.repetitions == 0 {
        bail!("token counts and repetitions must be non-zero");
    }
    let writes_greedy = args.greedy_tokens_output.is_some() || args.greedy_report_output.is_some();
    if writes_greedy && (args.greedy_tokens == 0 || args.greedy_tokens > args.generation_tokens) {
        bail!("greedy token count must be between one and the generation token count");
    }
    if args.greedy_only && !writes_greedy {
        bail!("--greedy-only requires a greedy output path");
    }
    let model = model::Model::load(
        &args.model_dir,
        args.prompt_tokens,
        args.generation_tokens,
        args.flash_attention,
    )?;
    let prompt = benchmark_tokens(args.prompt_tokens, model.vocab_size())?;
    let generation = benchmark_tokens(args.generation_tokens, model.vocab_size())?;

    if writes_greedy {
        let prompt = if let Some(path) = &args.greedy_prompt_tokens {
            serde_json::from_reader::<_, Vec<i32>>(std::fs::File::open(path)?)?
        } else {
            vec![GREEDY_SEED_TOKEN]
        };
        if prompt.is_empty() || prompt.len() + args.greedy_tokens > args.generation_tokens + 1 {
            bail!("greedy prompt and completion exceed the generation context");
        }
        let (tokens, elapsed) = greedy_tokens(
            &model,
            &prompt,
            args.greedy_tokens,
            args.greedy_prompt_tokens.is_some(),
        )?;
        if let Some(path) = &args.greedy_tokens_output {
            std::fs::write(path, serde_json::to_vec(&tokens)?)?;
        }
        if let Some(path) = &args.greedy_report_output {
            let generated_count = tokens.len();
            let report = json!({
                "tokens": tokens,
                "latency_ms": elapsed.as_secs_f64() * 1000.0,
                "tokens_per_second": generated_count as f64 / elapsed.as_secs_f64(),
            });
            std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
        }
    }
    if args.greedy_only {
        return Ok(());
    }

    model.clear_prompt_cache();
    model.run_prompt(&prompt)?;
    model.clear_generation_cache();
    model.run_generation_token(generation[0], 0)?;

    let prompt_result = benchmark_prompt(&model, &prompt, args.repetitions)?;
    let generation_result = benchmark_generation(&model, &generation, args.repetitions)?;
    let results = [prompt_result, generation_result];

    if args.json {
        let output = results
            .iter()
            .map(|result| {
                json!({
                    "test": result.name,
                    "tokens": result.tokens,
                    "repetitions": result.samples.len(),
                    "samples_ns": result.samples.iter().map(Duration::as_nanos).collect::<Vec<_>>(),
                    "samples_ts": result.tokens_per_second(),
                    "avg_ts": result.average_tokens_per_second(),
                    "stddev_ts": result.standard_deviation(),
                    "backend": "Metal",
                    "model": "Gemma 4 E2B Q4_K_M",
                    "cache_type_k": "f16",
                    "cache_type_v": "f16",
                    "flash_attention": args.flash_attention,
                })
            })
            .collect::<Vec<_>>();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("\n| test | tokens | t/s | stddev | backend |");
    println!("| --- | ---: | ---: | ---: | --- |");
    for result in results {
        println!(
            "| {} | {} | {:.2} | {:.2} | Metal |",
            result.name,
            result.tokens,
            result.average_tokens_per_second(),
            result.standard_deviation(),
        );
    }
    Ok(())
}

fn benchmark_prompt(
    model: &model::Model,
    token_ids: &[i32],
    repetitions: usize,
) -> Result<BenchmarkResult> {
    let mut samples = Vec::with_capacity(repetitions);
    for _ in 0..repetitions {
        model.clear_prompt_cache();
        let started = Instant::now();
        model.run_prompt(token_ids)?;
        samples.push(started.elapsed());
    }
    Ok(BenchmarkResult {
        name: format!("pp{}", token_ids.len()),
        tokens: token_ids.len(),
        samples,
    })
}

fn benchmark_generation(
    model: &model::Model,
    token_ids: &[i32],
    repetitions: usize,
) -> Result<BenchmarkResult> {
    let mut samples = Vec::with_capacity(repetitions);
    for _ in 0..repetitions {
        model.clear_generation_cache();
        let started = Instant::now();
        for (position, token_id) in token_ids.iter().enumerate() {
            model.run_generation_token(*token_id, position)?;
        }
        samples.push(started.elapsed());
    }
    Ok(BenchmarkResult {
        name: format!("tg{}", token_ids.len()),
        tokens: token_ids.len(),
        samples,
    })
}

fn greedy_tokens(
    model: &model::Model,
    prompt: &[i32],
    count: usize,
    stop_at_eos: bool,
) -> Result<(Vec<i32>, Duration)> {
    model.clear_generation_cache();
    model.run_generation_token(prompt[0], 0)?;
    let _ = model.generation_logits()?;
    model.clear_generation_cache();

    let started = Instant::now();
    for (position, token) in prompt.iter().enumerate() {
        model.run_generation_token(*token, position)?;
    }
    let mut token = greedy_argmax(model)?;
    let mut tokens = Vec::with_capacity(count);
    for index in 0..count {
        tokens.push(token);
        if stop_at_eos && (token == 1 || token == 106) {
            break;
        }
        if index + 1 < count {
            model.run_generation_token(token, prompt.len() + index)?;
            token = greedy_argmax(model)?;
        }
    }
    Ok((tokens, started.elapsed()))
}

fn greedy_argmax(model: &model::Model) -> Result<i32> {
    let logits = model.generation_logits()?;
    let mut index = 0;
    let first = logits.first().context("model returned no logits")?;
    let mut maximum = first;
    for (candidate_index, candidate) in logits.iter().enumerate().skip(1) {
        if candidate > maximum {
            index = candidate_index;
            maximum = candidate;
        }
    }
    i32::try_from(index).context("generated token ID exceeds i32")
}

fn benchmark_tokens(count: usize, vocab_size: usize) -> Result<Vec<i32>> {
    if vocab_size < 2 {
        bail!("model vocabulary is too small");
    }
    (0..count)
        .map(|index| {
            let token = (index.wrapping_mul(9_973).wrapping_add(17)) % vocab_size;
            i32::try_from(token).context("token ID exceeds i32")
        })
        .collect()
}

fn average(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn standard_deviation(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let mean = average(values);
    let variance = values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / (values.len() - 1) as f64;
    variance.sqrt()
}
