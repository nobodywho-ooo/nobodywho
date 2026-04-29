//! Inference benchmarks for nobodywho.
//!
//! Runs `ask()` at various conversation sizes. Tracing spans decompose the time
//! into prefill, generation, template rendering, tokenization, etc.
//! Produces a profiler summary table and optionally a flamegraph SVG.
//!
//! # Usage
//!
//! ```sh
//! # Basic run (GPU if available)
//! BENCH_MODEL=/path/to/model.gguf cargo bench --bench inference
//!
//! # CPU-only
//! BENCH_MODEL=/path/to/model.gguf BENCH_GPU=false cargo bench --bench inference
//!
//! # Generate a flamegraph SVG
//! BENCH_MODEL=/path/to/model.gguf FLAME_OUTPUT=flame.svg cargo bench --bench inference
//! ```

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use nobodywho::chat::{ChatBuilder, ChatHandle, Message};
use nobodywho::llm;
use nobodywho::profiling::SpanProfiler;
use nobodywho::sampler_config::SamplerPresets;
use std::sync::{Arc, OnceLock};
use tracing_subscriber::prelude::*;

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

/// Flush the flame layer and render the folded stacks into an SVG flamegraph.
fn write_flamegraph() {
    let Ok(flame_path) = std::env::var("FLAME_OUTPUT") else {
        return;
    };

    // Flush the flame layer
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

struct BenchState {
    #[allow(dead_code)]
    model: Arc<llm::Model>,
    chat: ChatHandle,
}

static BENCH_STATE: OnceLock<BenchState> = OnceLock::new();

fn get_bench_state() -> &'static BenchState {
    BENCH_STATE.get_or_init(|| {
        let model_path = std::env::var("BENCH_MODEL").expect(
            "BENCH_MODEL env var must be set to the path of a GGUF model file.\n\
             Example: BENCH_MODEL=/path/to/model.gguf cargo bench --bench inference",
        );
        let use_gpu = std::env::var("BENCH_GPU")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        eprintln!("Loading model: {model_path}");
        eprintln!("GPU: {}", if use_gpu { "enabled" } else { "disabled" });

        let model = Arc::new(
            llm::get_model(&model_path, use_gpu, None)
                .unwrap_or_else(|e| panic!("Failed to load model {model_path}: {e:?}")),
        );

        let chat = ChatBuilder::new(Arc::clone(&model))
            .with_context_size(4096)
            .with_system_prompt(Some("You are a helpful assistant. Be concise."))
            .with_template_variable("enable_thinking".to_string(), false)
            .with_sampler(SamplerPresets::greedy())
            .build();

        BenchState { model, chat }
    })
}

fn build_conversation(n_pairs: usize) -> Vec<Message> {
    let mut messages = Vec::with_capacity(n_pairs * 2);
    for i in 0..n_pairs {
        messages.push(Message::new_user(format!(
            "This is question number {}. Can you explain the history and significance \
             of number {} in mathematics, science, and culture?",
            i + 1,
            i + 1
        )));
        messages.push(Message::new_assistant(format!(
            "The number {} has many interesting properties. In mathematics, it appears \
             in various contexts including number theory and combinatorics. In science, \
             it relates to fundamental constants and measurements. Culturally, it holds \
             significance in many traditions around the world.",
            i + 1,
        )));
    }
    messages
}

fn bench_ask(c: &mut Criterion) {
    let profiler = init_tracing();
    let state = get_bench_state();

    let mut group = c.benchmark_group("ask");
    group.sample_size(10);

    for n_pairs in [0, 1, 5, 10, 20] {
        let conversation = build_conversation(n_pairs);
        let label = format!("{} messages", conversation.len());

        group.bench_with_input(
            BenchmarkId::new("history", &label),
            &conversation,
            |b, conversation| {
                b.iter(|| {
                    if !conversation.is_empty() {
                        state.chat.set_chat_history(conversation.clone()).unwrap();
                    } else {
                        state.chat.reset_history().unwrap();
                    }
                    state.chat.ask("What is 2+2?").completed().unwrap()
                });
            },
        );
    }

    group.finish();

    eprintln!("\n=== ask ===\n{}", profiler.summary());
    profiler.reset();
}

fn bench_long_generation(c: &mut Criterion) {
    let profiler = init_tracing();
    let state = get_bench_state();

    let mut group = c.benchmark_group("long_generation");
    group.sample_size(10);

    group.bench_function("count_to_20", |b| {
        b.iter(|| {
            state.chat.reset_history().unwrap();
            state.chat.ask("Count from 1 to 20.").completed().unwrap()
        });
    });

    group.finish();

    eprintln!("\n=== long_generation ===\n{}", profiler.summary());
    profiler.reset();

    write_flamegraph();
}

criterion_group!(benches, bench_ask, bench_long_generation);
criterion_main!(benches);
