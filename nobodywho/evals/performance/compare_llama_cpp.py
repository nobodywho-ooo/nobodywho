# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "nobodywho",
#   "llama-cpp-python",
# ]
#
# [tool.uv.sources]
# nobodywho = { path = "../../python" }
# ///

import time
from dataclasses import dataclass
from pathlib import Path
from typing import Literal

from llama_cpp import Llama

from nobodywho import Chat, Model, SamplerBuilder

# NobodyWho vs llama-cpp-python CPU benchmark
# Rebuild the local native extension after Rust changes with:
# uv run --reinstall-package nobodywho nobodywho/evals/performance/compare_llama_cpp.py
#
# Settings:
# - OUTPUT_CUTOFF=128 words (both backends stop after reaching the same cutoff)
# - ENABLE_REASONING=False
# - Sampler: temperature=0.0, top_p=0.95, top_k=64, min_p=0.0
# - 3 seeds per experiment, interleaved and counterbalanced by backend
# - device forced to CPU on both backends
# - generation words/s excludes time to first token (TTFT)

# To download model
# hf download hf://unsloth/gemma-4-E2B-it-GGUF/gemma-4-E2B-it-Q4_K_M.gguf --local-dir ./models
MODEL_PATH = Path(__file__).resolve().parents[3] / "models/gemma-4-E2B-it-Q4_K_M.gguf"
PROMPT = (
    "Write at least 300 words about Denmark's geography, history, government, "
    "economy, and culture."
)
OUTPUT_CUTOFF = 128
ENABLE_REASONING = False
TOP_K = 64
TOP_P = 0.95
MIN_P = 0.0
TEMPERATURE = 0.0
SEEDS = [42, 43, 44]

SamplingMode = Literal["model-default", "explicit-gemma4"]


@dataclass(frozen=True)
class Experiment:
    name: str
    backend: Literal["nobodywho", "llama-cpp-python"]
    sampling: SamplingMode


@dataclass(frozen=True)
class Result:
    experiment: Experiment
    seed: int
    word_count: int
    elapsed: float
    generation_elapsed: float
    generation_words_per_second: float
    ttft: float


EXPERIMENTS = [
    Experiment(
        name=f"{MODEL_PATH.name} via nobodywho",
        backend="nobodywho",
        sampling="explicit-gemma4",
    ),
    Experiment(
        name=f"{MODEL_PATH.name} via llama-cpp-python",
        backend="llama-cpp-python",
        sampling="explicit-gemma4",
    ),
]


def build_nobodywho_sampler(sampling: SamplingMode, seed: int):
    if sampling == "model-default":
        return None
    if sampling == "explicit-gemma4":
        return (
            SamplerBuilder()
            .top_k(TOP_K)
            .top_p(TOP_P, min_keep=1)
            .min_p(MIN_P, min_keep=1)
            .temperature(TEMPERATURE)
            .seed(seed)
            .dist()
        )
    raise ValueError(f"Unknown sampling mode: {sampling}")


def make_result(
    experiment: Experiment,
    seed: int,
    text: str,
    elapsed: float,
    ttft: float,
) -> Result:
    if ttft < 0:
        raise RuntimeError(f"{experiment.name} produced no output for seed {seed}")

    word_count = len(text.split())
    generation_elapsed = elapsed - ttft
    generation_words_per_second = (
        word_count / generation_elapsed if generation_elapsed > 0 else 0.0
    )
    return Result(
        experiment=experiment,
        seed=seed,
        word_count=word_count,
        elapsed=elapsed,
        generation_elapsed=generation_elapsed,
        generation_words_per_second=generation_words_per_second,
        ttft=ttft,
    )


def print_result(result: Result) -> None:
    print(
        f"\n\n[{result.experiment.name}] seed={result.seed} "
        f"{result.word_count} words in {result.generation_elapsed:.2f}s generation time "
        f"({result.generation_words_per_second:.2f} words/s, "
        f"ttft={result.ttft:.3f}s, total={result.elapsed:.2f}s)\n"
    )


def print_summary_table(results: list[Result]) -> None:
    from statistics import mean, stdev

    experiments_order = list(dict.fromkeys(r.experiment for r in results))
    headers = ("experiment", "seeds", "generation words/s ±σ", "ttft ±σ")

    rows: list[tuple[str, str, str, str]] = []
    for exp in experiments_order:
        exp_results = [r for r in results if r.experiment == exp]
        wps_values = [r.generation_words_per_second for r in exp_results]
        ttft_values = [r.ttft for r in exp_results]
        rows.append(
            (
                exp.name,
                str(len(exp_results)),
                f"{mean(wps_values):.2f} ± {stdev(wps_values):.2f}"
                if len(wps_values) > 1
                else f"{mean(wps_values):.2f}",
                f"{mean(ttft_values):.3f} ± {stdev(ttft_values):.3f}"
                if len(ttft_values) > 1
                else f"{mean(ttft_values):.3f}",
            )
        )

    widths = [
        max(len(headers[column]), *(len(row[column]) for row in rows))
        for column in range(len(headers))
    ]
    separator = "+-" + "-+".join("-" * (width + 2) for width in widths) + "-+"

    def format_row(row: tuple[str, str, str, str]) -> str:
        return (
            "| "
            + " | ".join(cell.rjust(widths[index]) for index, cell in enumerate(row))
            + " |"
        )

    print("\n=== summary ===\n")
    print(separator)
    print(format_row(headers))
    print(separator)
    for row in rows:
        print(format_row(row))
    print(separator)


def count_words_from_stream(text: str, pending: str) -> tuple[int, str]:
    pieces = (pending + text).split()
    if not pieces:
        return 0, ""
    if text and text[-1].isspace():
        return len(pieces), ""
    if pieces:
        return len(pieces) - 1, pieces[-1]
    return 0, ""


def run_nobodywho(experiment: Experiment, seed: int) -> Result:
    label = f"{experiment.name} seed={seed}"
    print(f"=== {label} ===\n")

    sampler = build_nobodywho_sampler(sampling=experiment.sampling, seed=seed)

    model = Model(str(MODEL_PATH), use_gpu_if_available=False)
    chat = Chat(
        model=model,
        sampler=sampler,
        template_variables={"enable_thinking": ENABLE_REASONING},
    )

    parts: list[str] = []
    completed_words = 0
    pending_text = ""
    ttft = -1.0
    start = time.perf_counter()

    for token in chat.ask(PROMPT):
        if ttft < 0:
            ttft = time.perf_counter() - start
        parts.append(token)
        new_words, pending_text = count_words_from_stream(token, pending_text)
        completed_words += new_words
        if completed_words >= OUTPUT_CUTOFF:
            chat.stop_generation()
            break

    elapsed = time.perf_counter() - start
    text = "".join(parts)
    print(text, end="", flush=True)
    result = make_result(
        experiment=experiment,
        seed=seed,
        text=text,
        elapsed=elapsed,
        ttft=ttft,
    )
    print_result(result=result)
    return result


def run_llama_cpp(experiment: Experiment, seed: int) -> Result:
    label = f"{experiment.name} seed={seed}"
    print(f"=== {label} ===\n")
    llm = Llama(
        model_path=str(MODEL_PATH),
        n_ctx=4096,
        n_gpu_layers=0,
        offload_kqv=False,
        op_offload=False,
        verbose=False,
        seed=seed,
    )

    parts: list[str] = []
    completed_words = 0
    pending_text = ""
    ttft = -1.0
    start = time.perf_counter()

    for chunk in llm.create_chat_completion(
        messages=[{"role": "user", "content": PROMPT}],
        max_tokens=2048,  # high ceiling; actual cutoff is word-based
        temperature=TEMPERATURE,
        top_p=TOP_P,
        top_k=TOP_K,
        min_p=MIN_P,
        stream=True,
    ):
        text = chunk["choices"][0]["delta"].get("content", "")
        if not text:
            continue
        if ttft < 0:
            ttft = time.perf_counter() - start
        parts.append(text)
        new_words, pending_text = count_words_from_stream(text, pending_text)
        completed_words += new_words
        if completed_words >= OUTPUT_CUTOFF:
            break

    elapsed = time.perf_counter() - start
    text = "".join(parts)
    print(text, end="", flush=True)
    result = make_result(
        experiment=experiment,
        seed=seed,
        text=text,
        elapsed=elapsed,
        ttft=ttft,
    )
    print_result(result=result)
    return result


def warmup() -> None:
    """Run a short inference on each backend so OS page cache is warm."""
    print("=== warmup (discarded) ===\n")

    model = Model(str(MODEL_PATH), use_gpu_if_available=False)
    chat = Chat(model=model, template_variables={"enable_thinking": ENABLE_REASONING})
    for _token in chat.ask("Hi"):
        pass
    print("[nobodywho] warmup done")

    llm = Llama(
        model_path=str(MODEL_PATH),
        n_ctx=4096,
        n_gpu_layers=0,
        offload_kqv=False,
        op_offload=False,
        verbose=False,
        seed=42,
    )
    for _chunk in llm.create_chat_completion(
        messages=[{"role": "user", "content": "Hi"}],
        max_tokens=8,
        temperature=TEMPERATURE,
        stream=True,
    ):
        pass
    print("[llama-cpp-python] warmup done\n")


def main() -> None:
    if not MODEL_PATH.exists():
        raise FileNotFoundError(f"Model not found: {MODEL_PATH}")

    warmup()

    runners = {
        "nobodywho": run_nobodywho,
        "llama-cpp-python": run_llama_cpp,
    }
    results: list[Result] = []
    for seed_index, seed in enumerate(SEEDS):
        experiments = EXPERIMENTS if seed_index % 2 == 0 else reversed(EXPERIMENTS)
        for experiment in experiments:
            results.append(
                runners[experiment.backend](experiment=experiment, seed=seed)
            )

    print_summary_table(results)


if __name__ == "__main__":
    main()
