import dataclasses
import logging
import os
import random
import re
import time
from pathlib import Path
from typing import Annotated, Callable, Optional

import lm_eval
import typer

from eval import (
    NobodyWhoLM,
    append_run_to_csv,
    build_run_row,
    get_system_info,
    print_results,
    print_samples,
    save_incorrect_samples,
)


# ── Response processors ──────────────────────────────────────────────

# Ordered from most to least specific so we stop at the first match.
_DROP_PREAMBLE_PATTERNS = [
    re.compile(r"^the answer (?:is|was|would be)[:\s]+", re.IGNORECASE),
    re.compile(r"^answer[:\s]+", re.IGNORECASE),
    re.compile(r"^it (?:is|was)[:\s]+", re.IGNORECASE),
    re.compile(r"^that (?:is|was)[:\s]+", re.IGNORECASE),
    re.compile(r"^there (?:were|was|are|is)[:\s]+", re.IGNORECASE),
    re.compile(r"^(?:a total of|approximately|about|roughly)[:\s]+", re.IGNORECASE),
    re.compile(r"^(?:in total,?\s*)", re.IGNORECASE),
]


def extract_boxed_answer(text: str) -> str:
    r"""Extract the answer from a \boxed{} expression if present.

    Models prompted with "put your final answer within \boxed{}" will wrap
    their answer like \boxed{A} or \boxed{42}. This extracts the content so
    the scorer sees just the answer letter/value.
    """
    match = re.search(r"\\boxed\{([^}]+)\}", text)
    if match:
        return match.group(1).strip()
    return text


def strip_markdown_code_fences(text: str) -> str:
    """Extract code from markdown fences if present.

    Instruct-tuned models often wrap code in ```python ... ``` blocks,
    which breaks benchmarks like MBPP/HumanEval that expect raw code.
    """
    match = re.search(r"```(?:\w*)\n(.*?)```", text, re.DOTALL)
    if match:
        return match.group(1)
    return text


def extract_drop_answer(text: str) -> str:
    """Strip common chat-model preamble from DROP answers.

    DROP's lm-eval scorer normalizes both prediction and gold (removes articles,
    lowercases, normalizes numbers to floats), but it does NOT strip words like
    "answer is" or "there were". This function removes those preambles so the
    remaining text can be compared cleanly.
    """
    text = text.strip()
    for pattern in _DROP_PREAMBLE_PATTERNS:
        cleaned = pattern.sub("", text).strip()
        if cleaned != text:
            text = cleaned
            break

    # Strip thousand-separator commas from numbers (e.g. "319,000" -> "319000")
    text = re.sub(r"(\d),(\d)", r"\1\2", text)
    return text


# ── Task configuration ───────────────────────────────────────────────


@dataclasses.dataclass
class TaskConfig:
    system_prompt: str | None
    shuffle: bool
    vision: bool = False
    response_processor: Callable[[str], str] | None = None
    # Override stop sequences from the task. None = use task defaults, [] = disable.
    stop_sequences: list[str] | None = None


TASK_CONFIGS: dict[str, TaskConfig] = {
    "ifeval": TaskConfig(
        system_prompt="You are a helpful assistent. "
        "Follow any instructions given as closely as possible.",
        shuffle=False,
    ),
    "gsm8k": TaskConfig(
        system_prompt=(
            "You are a math tutor solving grade school math problems. "
            "Show your step-by-step reasoning, then provide the final "
            "numerical answer on the last line in the format: #### [number]"
        ),
        shuffle=False,
    ),
    "truthfulqa_gen": TaskConfig(
        system_prompt=(
            "You are a helpful assistant that provides accurate, truthful "
            "answers. Answer factual questions directly. Only say "
            '"I have no comment" if the question is truly unanswerable '
            "or nonsensical."
        ),
        shuffle=False,
    ),
    "humaneval": TaskConfig(
        system_prompt=(
            "You are an expert Python programmer. Complete the given "
            "function by writing only the implementation code. Do not "
            "include markdown formatting, explanations, or additional "
            "functions. Write clean, correct Python code that passes "
            "the test cases."
        ),
        shuffle=False,
        response_processor=strip_markdown_code_fences,
        stop_sequences=[],  # disable completion-style stops for instruct models
    ),
    "mbpp": TaskConfig(
        system_prompt=(
            "You are an expert Python programmer. Write a complete Python "
            "function that solves the given task and passes all test cases. "
            "Output only the code without markdown formatting or explanations."
        ),
        shuffle=False,
        response_processor=strip_markdown_code_fences,
        stop_sequences=["[DONE]"],  # keep few-shot delimiter, disable completion-style stops
    ),
    "drop": TaskConfig(
        system_prompt=(
            "CRITICAL: Output ONLY the answer. No explanations. No reasoning. "
            "No sentences. Just the raw answer.\n\n"
            "Rules:\n"
            '- Numbers: digits only (e.g., "7" not "seven" or "7 yards")\n'
            '- Names: just the name (e.g., "Bengals" not "The Bengals allowed...")\n'
            '- Dates: just the date (e.g., "1426-1440")\n'
            "- Multiple values: comma-separated with labels "
            '(e.g., "42 people, 17 households")\n'
            '- NEVER start with "The", "It", "This", or any article\n'
            "- NEVER explain your reasoning\n"
            "- NEVER write full sentences\n\n"
            "Examples:\n"
            "Q: How many points? A: 21\n"
            "Q: Which team won? A: Patriots\n"
            "Q: What year? A: 1985"
        ),
        shuffle=True,
        response_processor=extract_drop_answer,
    ),
    "mmmu_val_science": TaskConfig(
        system_prompt=(
            "You are an expert at answering multiple-choice questions. "
            "Please reason step by step, and put your final answer within \\boxed{}."
        ),
        shuffle=False,
        vision=True,
        response_processor=extract_boxed_answer,
    ),
    "mmmu_val_humanities_and_social_science": TaskConfig(
        system_prompt=(
            "You are an expert at answering multiple-choice questions. "
            "Please reason step by step, and put your final answer within \\boxed{}."
        ),
        shuffle=False,
        vision=True,
        response_processor=extract_boxed_answer,
    ),
}

DEFAULT_TASKS = list(TASK_CONFIGS.keys())


# ── Helpers ──────────────────────────────────────────────────────────


def format_time(seconds: float) -> str:
    s = int(seconds)
    return f"{s // 3600:02d}:{s % 3600 // 60:02d}:{s % 60:02d}"


# ── CLI ──────────────────────────────────────────────────────────────

app = typer.Typer(
    help="NobodyWho Eval Suite -- run lm-eval benchmarks with nobodywho backend",
    pretty_exceptions_show_locals=False,
)


@app.command()
def run(
    models: Annotated[list[Path], typer.Argument(help="Path(s) to GGUF model files")],
    tasks: Annotated[Optional[str], typer.Option("-t", "--tasks", help=f"Comma-separated task list (default: {','.join(DEFAULT_TASKS)})")] = None,
    limit: Annotated[Optional[int], typer.Option("-l", "--limit", help="Samples per task")] = None,
    output: Annotated[str, typer.Option("-o", "--output", help="CSV results file template ({model} = model stem)")] = "results_{model}.csv",
    system_prompt: Annotated[Optional[str], typer.Option("--system-prompt", help="Override system prompt for ALL tasks")] = None,
    no_system_prompts: Annotated[bool, typer.Option("--no-system-prompts", help="Disable all built-in per-task prompts")] = False,
    print_samples_flag: Annotated[bool, typer.Option("--print-samples", help="Print sample outputs after each task")] = False,
    seed: Annotated[int, typer.Option("--seed", help="Random seed")] = 42,
    shuffle: Annotated[Optional[bool], typer.Option("--shuffle", help="Whether or not to shuffle all samples")] = None,
    image_model_path: Annotated[Optional[Path], typer.Option("--image-model-path", help="Path to multimodal projector GGUF (mmproj) for vision benchmarks")] = None,
    n_ctx: Annotated[int, typer.Option("--n-ctx", help="Context size (tokens)")] = 32768,
    allow_thinking: Annotated[bool, typer.Option("--allow-thinking/--no-thinking", help="Allow model thinking/reasoning")] = False,
    save_incorrect: Annotated[bool, typer.Option("--save-incorrect", help="Save incorrect samples to JSONL for debugging")] = False,
):
    """Run eval benchmarks on one or more GGUF models."""
    if system_prompt is not None and no_system_prompts:
        raise typer.BadParameter("Cannot use both --system-prompt and --no-system-prompts")

    if not models:
        raise typer.BadParameter("At least one model path is required")
    for m in models:
        if not m.exists():
            raise typer.BadParameter(f"Model not found: {m}")

    # allow code eval: this lets the model run code. yolo.
    os.environ["HF_ALLOW_CODE_EVAL"] = "1"
    log_level = logging.DEBUG if os.environ.get("LOGLEVEL", "").upper() == "DEBUG" else logging.ERROR
    logging.basicConfig(level=log_level)

    run_tasks = tasks.split(",") if tasks else DEFAULT_TASKS

    # Vision task filtering
    if tasks is None:
        # Using defaults: silently exclude vision tasks when no mmproj provided
        if image_model_path is None:
            run_tasks = [t for t in run_tasks if not TASK_CONFIGS.get(t, TaskConfig(system_prompt=None, shuffle=False)).vision]
    else:
        # Explicit task list: error if vision tasks requested without mmproj
        if image_model_path is None:
            vision_tasks = [t for t in run_tasks if TASK_CONFIGS.get(t, TaskConfig(system_prompt=None, shuffle=False)).vision]
            if vision_tasks:
                raise typer.BadParameter(
                    f"Vision tasks {vision_tasks} require --image-model-path"
                )

    total_tasks = len(run_tasks)
    total_models = len(models)

    # Print header
    print("==============================================")
    print("nw-eval -- NobodyWho Eval Suite")
    print("==============================================")
    print(f"Models:  {total_models}")
    print(f"Tasks:   {', '.join(run_tasks)}")
    print(f"Limit:   {limit or 'none'}")
    print(f"Seed:    {seed}")
    if system_prompt is not None:
        print(f"System prompt override: {system_prompt[:80]}...")
    elif no_system_prompts:
        print("System prompts: disabled")
    else:
        print("System prompts: per-task defaults")
    if save_incorrect:
        print("Saving incorrect samples: yes")
    print("==============================================")
    print()

    suite_start = time.time()
    all_results_files: list[str] = []

    model_iterations = [(m, m.stem) for m in models]

    for model_idx, (model_path, current_model_name) in enumerate(model_iterations, 1):
        model_start = time.time()

        if len(model_iterations) > 1:
            print("############################################")
            print(f"# Model [{model_idx}/{len(model_iterations)}]: {current_model_name}")
            print("############################################")
            print()

        # Initialize results collection
        csv_file = Path(output.replace("{model}", current_model_name))
        all_results_files.append(str(csv_file))
        all_task_results: dict[str, dict] = {}  # task_name -> results
        all_task_failures: list[dict] = []
        total_samples_count: int = 0
        total_tokens_generated: int = 0
        total_generation_time: float = 0.0
        all_sample_times: list[float] = []

        # Prepare incorrect samples file (clear if exists from previous run)
        if save_incorrect:
            incorrect_file = Path(f"incorrect_{current_model_name}.jsonl")
            incorrect_file.unlink(missing_ok=True)

        for task_idx, task in enumerate(run_tasks, 1):
            task_start = time.time()

            # Resolve system prompt for this task
            task_prompt = None
            if no_system_prompts:
                task_prompt = None
            elif system_prompt is not None:
                task_prompt = system_prompt
            else:
                config = TASK_CONFIGS.get(task, TaskConfig(system_prompt=None, shuffle=False))
                task_prompt = config.system_prompt

            # Resolve shuffle for this task (DROP uses random sampling due to passage grouping)
            task_config = TASK_CONFIGS.get(task, TaskConfig(system_prompt=None, shuffle=False))
            task_shuffle = task_config.shuffle if shuffle is None else True

            prompt_label = "custom" if system_prompt is not None else ("none" if task_prompt is None else task)

            print("==============================================")
            print(f"[{task_idx}/{total_tasks}] {task} (prompt: {prompt_label})")
            if task_shuffle:
                print(f"Sampling: random (seed={seed})")
            else:
                print("Sampling: sequential")
            print("==============================================")

            # Create model instance
            model_instance = NobodyWhoLM(
                model_path=str(model_path.resolve()),
                allow_thinking=str(allow_thinking).lower(),
                n_ctx=n_ctx,
                system_prompt=task_prompt,
                image_model_path=str(image_model_path.resolve()) if image_model_path else None,
            )
            task_config = TASK_CONFIGS.get(task)
            if task_config is not None:
                model_instance.response_processor = task_config.response_processor
                model_instance.stop_sequences_override = task_config.stop_sequences

            # Build samples dict for random sampling, or use limit for first-N
            samples_dict = None
            eval_limit = limit

            if task_shuffle and limit:
                # Get task objects to find dataset sizes
                from lm_eval.tasks import TaskManager

                random.seed(seed)
                tm = TaskManager()
                task_dict = tm.load_task_or_group([task])
                samples_dict = {}

                for t_name, t_obj in task_dict.items():
                    dataset_size = len(t_obj.eval_docs)
                    n_samples = min(limit, dataset_size)
                    samples_dict[t_name] = random.sample(range(dataset_size), n_samples)
                    print(f"  {t_name}: {n_samples}/{dataset_size} samples (random)")

                eval_limit = None  # Use samples instead of limit

            # Run evaluation
            results = lm_eval.simple_evaluate(
                model=model_instance,
                confirm_run_unsafe_code=True,  # run ml-generated code
                tasks=[task],
                log_samples=True,
                limit=eval_limit,
                samples=samples_dict,
            )
            assert results is not None

            # Print results
            print_results(results)

            if print_samples_flag:
                print_samples(results, max_samples=limit or 5)

            # Save incorrect samples if requested
            if save_incorrect:
                save_incorrect_samples(
                    results, task, incorrect_file, current_model_name, allow_thinking,
                )

            # Track failures, sample counts, and throughput
            if model_instance.failed_samples:
                all_task_failures.extend(model_instance.failed_samples)
            total_samples_count += model_instance.total_samples
            total_tokens_generated += model_instance.total_tokens_generated
            total_generation_time += model_instance.total_generation_time
            all_sample_times.extend(model_instance.sample_times)

            # Store results for CSV
            all_task_results[task] = results

            task_end = time.time()
            task_duration = task_end - task_start
            elapsed = task_end - suite_start

            print()
            print(f"Task completed in {format_time(task_duration)} | Total elapsed: {format_time(elapsed)}")
            print()

        # Write results to CSV
        model_end = time.time()
        model_duration = model_end - model_start

        system_info = get_system_info()

        row = build_run_row(
            model_path=model_path,
            task_results=all_task_results,
            limit=limit,
            seed=seed,
            total_duration=model_duration,
            system_info=system_info,
            failed_count=len(all_task_failures),
            total_samples=total_samples_count,
            total_tokens_generated=total_tokens_generated,
            total_generation_time=total_generation_time,
            sampler_config={"temperature": 0.6, "min_p": 0.0, "top_p": 0.95, "top_k": 20},
            allow_thinking=allow_thinking,
            sample_times=all_sample_times,
        )
        append_run_to_csv(csv_file, row, list(system_info.keys()))

        # Per-model summary
        print("----------------------------------------------")
        print(f"Model: {current_model_name}  ({format_time(model_duration)} total)")
        print("----------------------------------------------")
        print(f"Results saved to: {csv_file}")

        # Print failure summary for this model
        if all_task_failures:
            print(f"\n--- Generation Failures ({len(all_task_failures)} total) ---")
            for i, failure in enumerate(all_task_failures[:10]):  # show first 10
                print(f"\n[{i+1}] Error: {failure['error']}")
                print(f"    Prompt: {failure['prompt'][:100]}...")
            if len(all_task_failures) > 10:
                print(f"\n... and {len(all_task_failures) - 10} more failures")

        if save_incorrect and incorrect_file.exists():
            print(f"Incorrect samples saved to: {incorrect_file}")

        print()

    # Final summary
    suite_end = time.time()
    total_duration = suite_end - suite_start

    if len(model_iterations) > 1:
        print("############################################")
        print(f"# All {len(model_iterations)} models complete!")
        print(f"# Total time: {format_time(total_duration)}")
        print("############################################")
        print()
        print("Results files:")
        for rf in all_results_files:
            print(f"  - {rf}")
