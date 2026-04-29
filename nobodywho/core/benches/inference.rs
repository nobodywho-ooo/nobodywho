//! Inference benchmarks for nobodywho.
//!
//! Measures prompt processing (pp) and text generation (tg) performance,
//! outputting a markdown table similar to llama-bench. Results can be
//! appended as JSONL for regression tracking across commits.
//!
//! # Usage
//!
//! ```sh
//! # Basic run (GPU if available)
//! BENCH_MODEL=/path/to/model.gguf cargo bench --bench inference
//!
//! # CPU-only, save results for regression tracking
//! BENCH_MODEL=/path/to/model.gguf BENCH_GPU=false \
//!   BENCH_OUTPUT=bench_results.jsonl cargo bench --bench inference
//!
//! # Generate a flamegraph SVG
//! BENCH_MODEL=/path/to/model.gguf FLAME_OUTPUT=flame.svg cargo bench --bench inference
//! ```
//!
//! # Environment variables
//!
//! - `BENCH_MODEL` (required) — path to a GGUF model file
//! - `BENCH_GPU` — set to `false` or `0` to disable GPU (default: enabled)
//! - `BENCH_SAMPLES` — number of iterations per test (default: 10)
//! - `BENCH_OUTPUT` — path to append JSONL results (default: none)
//! - `FLAME_OUTPUT` — path to write flamegraph SVG (default: none)

use nobodywho::chat::{ChatBuilder, ChatHandle, Message};
use nobodywho::llm;
use nobodywho::profiling::SpanProfiler;
use nobodywho::sampler_config::SamplerPresets;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
use std::time::Instant;
use tracing_subscriber::prelude::*;

// --- Tracing & Flamegraph ---

static PROFILER: OnceLock<SpanProfiler> = OnceLock::new();
static FLAME_GUARD: OnceLock<
    std::sync::Mutex<Option<tracing_flame::FlushGuard<std::io::BufWriter<std::fs::File>>>>,
> = OnceLock::new();

fn init_tracing() -> &'static SpanProfiler {
    PROFILER.get_or_init(|| {
        let profiler = SpanProfiler::new();

        let flame_layer = std::env::var("FLAME_OUTPUT").ok().map(|_| {
            let (layer, guard) =
                tracing_flame::FlameLayer::with_file("tracing-flame.folded").unwrap();
            let _ = FLAME_GUARD.set(std::sync::Mutex::new(Some(guard)));
            layer
        });

        tracing_subscriber::registry()
            .with(profiler.layer())
            .with(tracing_subscriber::filter::LevelFilter::DEBUG)
            .with(flame_layer)
            .init();

        profiler
    })
}

fn write_flamegraph() {
    let Ok(flame_path) = std::env::var("FLAME_OUTPUT") else {
        return;
    };
    if let Some(guard_mutex) = FLAME_GUARD.get() {
        if let Some(guard) = guard_mutex.lock().unwrap().take() {
            drop(guard);
        }
    }
    let folded_path = "tracing-flame.folded";
    if std::path::Path::new(folded_path).exists() {
        let folded = std::fs::File::open(folded_path).unwrap();
        let reader = std::io::BufReader::new(folded);
        let mut svg_file = std::fs::File::create(&flame_path).unwrap();
        let mut opts = inferno::flamegraph::Options::default();
        opts.title = "nobodywho inference flamegraph".to_string();
        inferno::flamegraph::from_reader(&mut opts, reader, &mut svg_file).unwrap();
        let _ = std::fs::remove_file(folded_path);
        eprintln!("Flamegraph written to: {flame_path}");
    }
}

// --- Bench State ---

struct BenchState {
    model_path: String,
    gpu: bool,
    #[allow(dead_code)]
    model: Arc<llm::Model>,
    chat: ChatHandle,
}

fn init_bench_state() -> BenchState {
    let model_path = std::env::var("BENCH_MODEL").expect(
        "BENCH_MODEL env var must be set to the path of a GGUF model file.\n\
         Example: BENCH_MODEL=/path/to/model.gguf cargo bench --bench inference",
    );
    let gpu = std::env::var("BENCH_GPU")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);

    eprintln!("Loading model: {model_path}");
    eprintln!("GPU: {}", if gpu { "enabled" } else { "disabled" });

    let model = Arc::new(
        llm::get_model(&model_path, gpu, None, None)
            .unwrap_or_else(|e| panic!("Failed to load model {model_path}: {e:?}")),
    );

    let chat = ChatBuilder::new(Arc::clone(&model))
        .with_context_size(4096)
        .with_system_prompt(Some("You are a helpful assistant. Be concise."))
        .with_template_variable("enable_thinking".to_string(), false)
        .with_sampler(SamplerPresets::greedy())
        .build();

    BenchState {
        model_path,
        gpu,
        model,
        chat,
    }
}

// --- Measurement ---

struct IterationStats {
    wall_ms: f64,
    prefill_ms: f64,
    tg_tokens: u64,
    tg_ms: f64,
}

fn run_iteration(
    state: &BenchState,
    profiler: &SpanProfiler,
    history: &[Message],
    prompt: &str,
) -> IterationStats {
    profiler.reset();

    if !history.is_empty() {
        state.chat.set_chat_history(history.to_vec()).unwrap();
    } else {
        state.chat.reset_history().unwrap();
    }

    let wall_start = Instant::now();
    let _response = state.chat.ask(prompt).completed().unwrap();
    let wall_ms = wall_start.elapsed().as_secs_f64() * 1000.0;

    let stats = profiler.stats();

    let prefill_ns = stats
        .iter()
        .find(|s| s.name == "llm::prefill_decode")
        .map(|s| s.total_ns)
        .unwrap_or(0);

    let (tg_tokens, tg_ns) = stats
        .iter()
        .find(|s| s.name == "llm::write_decode")
        .map(|s| (s.count, s.total_ns))
        .unwrap_or((0, 0));

    IterationStats {
        wall_ms,
        prefill_ms: prefill_ns as f64 / 1_000_000.0,
        tg_tokens,
        tg_ms: tg_ns as f64 / 1_000_000.0,
    }
}

fn mean_stddev(values: &[f64]) -> (f64, f64) {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    (mean, variance.sqrt())
}

// --- Results ---

#[derive(Clone, Serialize, Deserialize)]
struct BenchResult {
    git_commit: String,
    timestamp: String,
    model: String,
    gpu: bool,
    test: String,
    n_messages: usize,
    samples: usize,
    avg_tg_ts: f64,
    stddev_tg_ts: f64,
    avg_pp_ms: f64,
    stddev_pp_ms: f64,
    avg_wall_ms: f64,
    stddev_wall_ms: f64,
}

fn git_commit_short() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn build_conversation(n_pairs: usize) -> Vec<Message> {
    let mut messages = Vec::with_capacity(n_pairs * 2);
    for i in 0..n_pairs {
        messages.push(Message::new_user(format!("Question {}", i + 1)));
        messages.push(Message::new_assistant(format!("Answer {}", i + 1)));
    }
    messages
}

fn run_test(
    state: &BenchState,
    profiler: &SpanProfiler,
    name: &str,
    history: &[Message],
    prompt: &str,
    n_samples: usize,
) -> BenchResult {
    // Warmup
    eprintln!("  warmup: {name}");
    run_iteration(state, profiler, history, prompt);

    let mut pp_ms_vals = Vec::with_capacity(n_samples);
    let mut tg_ts_vals = Vec::with_capacity(n_samples);
    let mut wall_ms_vals = Vec::with_capacity(n_samples);

    for i in 0..n_samples {
        eprint!("  sample {}/{n_samples}\r", i + 1);
        let iter = run_iteration(state, profiler, history, prompt);
        pp_ms_vals.push(iter.prefill_ms);
        wall_ms_vals.push(iter.wall_ms);
        if iter.tg_ms > 0.0 {
            tg_ts_vals.push(iter.tg_tokens as f64 / (iter.tg_ms / 1000.0));
        }
    }
    eprintln!();

    let (avg_pp_ms, stddev_pp_ms) = mean_stddev(&pp_ms_vals);
    let (avg_tg_ts, stddev_tg_ts) = if tg_ts_vals.is_empty() {
        (0.0, 0.0)
    } else {
        mean_stddev(&tg_ts_vals)
    };
    let (avg_wall_ms, stddev_wall_ms) = mean_stddev(&wall_ms_vals);

    BenchResult {
        git_commit: git_commit_short(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        model: state.model_path.clone(),
        gpu: state.gpu,
        test: name.to_string(),
        n_messages: history.len(),
        samples: n_samples,
        avg_tg_ts,
        stddev_tg_ts,
        avg_pp_ms,
        stddev_pp_ms,
        avg_wall_ms,
        stddev_wall_ms,
    }
}

// --- Output ---

fn print_table(results: &[BenchResult]) {
    eprintln!(
        "| {:<16} | {:>6} | {:>7} | {:>18} | {:>18} | {:>18} |",
        "test", "n_msgs", "samples", "pp ms", "tg t/s", "wall ms"
    );
    eprintln!(
        "|{:-<18}|{:-<8}|{:-<9}|{:-<20}|{:-<20}|{:-<20}|",
        "", "", "", "", "", ""
    );
    for r in results {
        eprintln!(
            "| {:<16} | {:>6} | {:>7} | {:>8.2} ± {:<7.2} | {:>8.2} ± {:<7.2} | {:>8.2} ± {:<7.2} |",
            r.test,
            r.n_messages,
            r.samples,
            r.avg_pp_ms,
            r.stddev_pp_ms,
            r.avg_tg_ts,
            r.stddev_tg_ts,
            r.avg_wall_ms,
            r.stddev_wall_ms,
        );
    }
}

fn append_jsonl(results: &[BenchResult], path: &str) {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap_or_else(|e| panic!("Failed to open {path}: {e}"));

    for r in results {
        let line = serde_json::to_string(r).unwrap();
        writeln!(file, "{line}").unwrap();
    }
    eprintln!("Results appended to: {path}");
}

fn compare_with_previous(results: &[BenchResult], path: &str) {
    let contents = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return, // no previous results
    };

    let previous: Vec<BenchResult> = contents
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    if previous.is_empty() {
        return;
    }

    eprintln!();
    eprintln!("Comparison with previous run:");
    eprintln!(
        "| {:<16} | {:>6} | {:>12} | {:>12} |",
        "test", "n_msgs", "tg t/s delta", "pp ms delta"
    );
    eprintln!(
        "|{:-<18}|{:-<8}|{:-<14}|{:-<14}|",
        "", "", "", ""
    );

    for r in results {
        // Find the most recent previous result with same test + n_messages + model
        let prev = previous
            .iter()
            .rev()
            .find(|p| p.test == r.test && p.n_messages == r.n_messages && p.model == r.model);

        let (tg_delta, pp_delta) = match prev {
            Some(p) => {
                let tg = if p.avg_tg_ts > 0.0 {
                    format!("{:+.1}%", (r.avg_tg_ts - p.avg_tg_ts) / p.avg_tg_ts * 100.0)
                } else {
                    "n/a".to_string()
                };
                let pp = if p.avg_pp_ms > 0.0 {
                    // negative = faster (less ms), so flip sign for readability
                    format!("{:+.1}%", (r.avg_pp_ms - p.avg_pp_ms) / p.avg_pp_ms * 100.0)
                } else {
                    "n/a".to_string()
                };
                (tg, pp)
            }
            None => ("baseline".to_string(), "baseline".to_string()),
        };

        eprintln!(
            "| {:<16} | {:>6} | {:>12} | {:>12} |",
            r.test, r.n_messages, tg_delta, pp_delta
        );
    }
}

// --- Main ---

fn main() {
    let profiler = init_tracing();
    let state = init_bench_state();
    let n_samples: usize = std::env::var("BENCH_SAMPLES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    eprintln!("Samples per test: {n_samples}");
    eprintln!();

    let mut results = Vec::new();

    // pp tests: increasing history sizes, short generation
    for n_pairs in [1, 5, 10, 20] {
        let history = build_conversation(n_pairs);
        let name = format!("pp {}msg", history.len());
        results.push(run_test(
            &state, profiler, &name, &history, "ok", n_samples,
        ));
    }

    // tg test: empty history, long generation
    results.push(run_test(
        &state,
        profiler,
        "tg",
        &[],
        "Count from 1 to 100.",
        n_samples,
    ));

    eprintln!();
    print_table(&results);

    // JSONL output + comparison
    if let Ok(path) = std::env::var("BENCH_OUTPUT") {
        compare_with_previous(&results, &path);
        append_jsonl(&results, &path);
    }

    // Span profiler detail (from last test)
    eprintln!();
    eprintln!("Span profiler (last test):");
    eprintln!("{}", profiler.summary());

    write_flamegraph();
}
