# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "nobodywho",
#   "llama-cpp-python",
# ]
#
# [tool.uv.sources]
# nobodywho = { git = "https://github.com/nobodywho-ooo/nobodywho", branch = "main", subdirectory = "nobodywho/python" }
# ///

import argparse
import random
import time
from dataclasses import dataclass
from pathlib import Path
from statistics import mean, stdev
from typing import Literal

from llama_cpp import Llama

from nobodywho import Chat, Model, SamplerBuilder

# NobodyWho vs llama-cpp-python CPU benchmark
# Run against the latest NobodyWho `main` revision with:
# uv run --refresh-package nobodywho nobodywho/evals/performance/compare_llama_cpp.py --model path/to/model.gguf --num-seeds 3
# To test another branch, change `branch` in `[tool.uv.sources]` above and rerun.
#
# Settings:
# - OUTPUT_CUTOFF=128 words (both backends stop after reaching the same cutoff)
# - ENABLE_REASONING=False
# - Sampler: temperature=0.0, top_p=0.95, top_k=64, min_p=0.0
# - deterministic random seeds, interleaved and counterbalanced by backend
# - device forced to CPU on both backends
# - generation throughput and latency exclude the first token and TTFT

# To download model
# hf download hf://unsloth/gemma-4-E2B-it-GGUF/gemma-4-E2B-it-Q4_K_M.gguf --local-dir ./models
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
SEED_GENERATOR_SEED = 42

SamplingMode = Literal["model-default", "explicit-gemma4"]
BACKEND_LABELS = {
    "nobodywho": "NobodyWho",
    "llama-cpp-python": "llama-cpp-python",
}


@dataclass(frozen=True)
class Experiment:
    name: str
    model_path: Path
    backend: Literal["nobodywho", "llama-cpp-python"]
    sampling: SamplingMode


@dataclass(frozen=True)
class Result:
    experiment: Experiment
    seed: int
    word_count: int
    token_count: int
    elapsed: float
    elapsed_after_first_token: float
    words_per_second_after_first: float
    tokens_per_second_after_first: float
    milliseconds_per_token_after_first: float
    ttft: float


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed < 1:
        raise argparse.ArgumentTypeError("must be at least 1")
    return parsed


def generate_seeds(num_seeds: int) -> list[int]:
    randomizer = random.Random(x=SEED_GENERATOR_SEED)
    return [randomizer.randint(a=0, b=2**31 - 1) for _ in range(num_seeds)]


def build_experiments(model_path: Path) -> list[Experiment]:
    return [
        Experiment(
            name=f"{model_path.name} via nobodywho",
            model_path=model_path,
            backend="nobodywho",
            sampling="explicit-gemma4",
        ),
        Experiment(
            name=f"{model_path.name} via llama-cpp-python",
            model_path=model_path,
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
    token_count: int,
    elapsed: float,
    elapsed_after_first_token: float,
    ttft: float,
) -> Result:
    if ttft < 0:
        raise RuntimeError(f"{experiment.name} produced no output for seed {seed}")

    word_count = len(text.split())
    tokens_after_first = max(token_count - 1, 0)
    words_per_second_after_first = (
        word_count / elapsed_after_first_token if elapsed_after_first_token > 0 else 0.0
    )
    tokens_per_second_after_first = (
        tokens_after_first / elapsed_after_first_token
        if elapsed_after_first_token > 0
        else 0.0
    )
    milliseconds_per_token_after_first = (
        elapsed_after_first_token / tokens_after_first * 1000
        if tokens_after_first > 0
        else 0.0
    )
    return Result(
        experiment=experiment,
        seed=seed,
        word_count=word_count,
        token_count=token_count,
        elapsed=elapsed,
        elapsed_after_first_token=elapsed_after_first_token,
        words_per_second_after_first=words_per_second_after_first,
        tokens_per_second_after_first=tokens_per_second_after_first,
        milliseconds_per_token_after_first=milliseconds_per_token_after_first,
        ttft=ttft,
    )


def print_result(result: Result) -> None:
    label = f"{BACKEND_LABELS[result.experiment.backend]} | seed={result.seed}"
    print(
        f"\n=== End generation: {label} ===\n"
        f"{result.word_count} words | {result.token_count} tokens | "
        f"after first: {result.words_per_second_after_first:.2f} words/s, "
        f"{result.tokens_per_second_after_first:.2f} tokens/s, "
        f"{result.milliseconds_per_token_after_first:.2f} ms/token | "
        f"TTFT: {result.ttft:.3f}s | total: {result.elapsed:.2f}s\n",
        flush=True,
    )


def format_distribution(values: list[float], decimals: int) -> str:
    average = mean(values)
    if len(values) == 1:
        return f"{average:.{decimals}f}"
    return f"{average:.{decimals}f} ± {stdev(values):.{decimals}f}"


def print_summary(results: list[Result]) -> None:
    experiments = list(dict.fromkeys(result.experiment for result in results))
    grouped_results = [
        (experiment, [result for result in results if result.experiment == experiment])
        for experiment in experiments
    ]
    headers = [
        "Metric",
        *[BACKEND_LABELS[experiment.backend] for experiment, _ in grouped_results],
    ]
    rows = [
        [
            "Tokens/s after first",
            *[
                format_distribution(
                    values=[result.tokens_per_second_after_first for result in group],
                    decimals=2,
                )
                for _, group in grouped_results
            ],
        ],
        [
            "ms/token after first",
            *[
                format_distribution(
                    values=[
                        result.milliseconds_per_token_after_first for result in group
                    ],
                    decimals=2,
                )
                for _, group in grouped_results
            ],
        ],
        [
            "TTFT (s)",
            *[
                format_distribution(
                    values=[result.ttft for result in group],
                    decimals=3,
                )
                for _, group in grouped_results
            ],
        ],
    ]
    widths = [
        max(len(headers[column]), *(len(row[column]) for row in rows))
        for column in range(len(headers))
    ]

    def format_row(row: list[str]) -> str:
        return "  ".join(
            cell.ljust(widths[index]) if index == 0 else cell.rjust(widths[index])
            for index, cell in enumerate(row)
        )

    seed_count = len(grouped_results[0][1])
    model_name = results[0].experiment.model_path.name
    print("=== Results ===")
    print(f"Model: {model_name}")
    print(f"Total seeds: {seed_count}")
    print("\nComparison:")
    print(format_row(row=headers))
    for row in rows:
        print(format_row(row=row))
    print("=== End results ===")


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
    sampler = build_nobodywho_sampler(sampling=experiment.sampling, seed=seed)

    model = Model(str(experiment.model_path), use_gpu_if_available=False)
    chat = Chat(
        model=model,
        sampler=sampler,
        template_variables={"enable_thinking": ENABLE_REASONING},
    )

    label = f"{BACKEND_LABELS[experiment.backend]} | seed={seed}"
    print(f"=== Start generation: {label} ===\n", flush=True)
    parts: list[str] = []
    completed_words = 0
    pending_text = ""
    ttft = -1.0
    last_token_elapsed = -1.0
    start = time.perf_counter()

    for token in chat.ask(PROMPT):
        last_token_elapsed = time.perf_counter() - start
        if ttft < 0:
            ttft = last_token_elapsed
        parts.append(token)
        new_words, pending_text = count_words_from_stream(token, pending_text)
        completed_words += new_words
        if completed_words >= OUTPUT_CUTOFF:
            chat.stop_generation()
            break

    elapsed = time.perf_counter() - start
    text = "".join(parts)
    print(text, end="", flush=True)
    # Chat.tokenize follows the model's BOS setting, so remove the empty baseline.
    token_count = len(chat.tokenize(prompt=text)) - len(chat.tokenize(prompt=""))
    result = make_result(
        experiment=experiment,
        seed=seed,
        text=text,
        token_count=token_count,
        elapsed=elapsed,
        elapsed_after_first_token=last_token_elapsed - ttft,
        ttft=ttft,
    )
    print_result(result=result)
    return result


def run_llama_cpp(experiment: Experiment, seed: int) -> Result:
    llm = Llama(
        model_path=str(experiment.model_path),
        n_ctx=4096,
        n_gpu_layers=0,
        offload_kqv=False,
        op_offload=False,
        verbose=False,
        seed=seed,
    )

    label = f"{BACKEND_LABELS[experiment.backend]} | seed={seed}"
    print(f"=== Start generation: {label} ===\n", flush=True)
    parts: list[str] = []
    completed_words = 0
    pending_text = ""
    ttft = -1.0
    last_token_elapsed = -1.0
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
        last_token_elapsed = time.perf_counter() - start
        if ttft < 0:
            ttft = last_token_elapsed
        parts.append(text)
        new_words, pending_text = count_words_from_stream(text, pending_text)
        completed_words += new_words
        if completed_words >= OUTPUT_CUTOFF:
            break

    elapsed = time.perf_counter() - start
    text = "".join(parts)
    print(text, end="", flush=True)
    token_count = len(llm.tokenize(text=text.encode(), add_bos=False))
    result = make_result(
        experiment=experiment,
        seed=seed,
        text=text,
        token_count=token_count,
        elapsed=elapsed,
        elapsed_after_first_token=last_token_elapsed - ttft,
        ttft=ttft,
    )
    print_result(result=result)
    return result


def warmup(model_path: Path) -> None:
    """Run a short inference on each backend so OS page cache is warm."""
    print("=== Warmup ===", flush=True)

    model = Model(str(model_path), use_gpu_if_available=False)
    chat = Chat(model=model, template_variables={"enable_thinking": ENABLE_REASONING})
    for _token in chat.ask("Hi"):
        pass
    print("[nobodywho] warmup done", flush=True)

    llm = Llama(
        model_path=str(model_path),
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
    print("[llama-cpp-python] warmup done", flush=True)
    print("=== End warmup ===\n", flush=True)


def main(model: Path, num_seeds: int = 3) -> None:
    if not model.exists():
        raise FileNotFoundError(f"Model not found: {model}")

    warmup(model_path=model)

    runners = {
        "nobodywho": run_nobodywho,
        "llama-cpp-python": run_llama_cpp,
    }
    all_experiments = build_experiments(model_path=model)
    seeds = generate_seeds(num_seeds=num_seeds)
    results: list[Result] = []
    for seed_index, seed in enumerate(seeds):
        experiments = (
            all_experiments if seed_index % 2 == 0 else reversed(all_experiments)
        )
        for experiment in experiments:
            results.append(
                runners[experiment.backend](experiment=experiment, seed=seed)
            )

    print_summary(results=results)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Benchmark NobodyWho against llama-cpp-python on CPU."
    )
    parser.add_argument(
        "--model", type=Path, required=True, help="Path to a GGUF model"
    )
    parser.add_argument(
        "--num-seeds",
        type=positive_int,
        default=3,
        help="Number of deterministic random seeds (default: 3)",
    )
    arguments = parser.parse_args()
    main(model=arguments.model, num_seeds=arguments.num_seeds)
