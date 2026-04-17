import json
import logging
import os
import platform
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

import lm_eval
import nobodywho
import psutil
from lm_eval.api.instance import Instance
from lm_eval.api.model import LM
from lm_eval.api.registry import register_model
from tqdm import tqdm

logger = logging.getLogger(__name__)


# ── Think-block helpers ──────────────────────────────────────────────
# Standard models (DeepSeek, Qwen, …) use <think>…</think>.
# Gemma 4 uses <|channel>thought\n…\n<channel|>.

# Markers that signal the end of a thinking block in the token stream.
THINK_END_MARKERS = ["</think>", "<channel|>"]


def strip_think_block(text: str) -> tuple[str, str]:
    """Strip the thinking block from a full model response.

    Returns (thinking_text, response_text).
    Handles both <think>…</think> and <|channel>thought…<channel|> formats.
    Splits on the *last* occurrence so nested/multiple blocks are handled.
    """
    # Standard format: <think>…</think>
    if "</think>" in text:
        before, after = text.rsplit("</think>", 1)
        thinking = before.replace("<think>", "").strip()
        return thinking, after.lstrip()

    # Gemma 4 format: <|channel>thought\n…\n<channel|>
    if "<channel|>" in text:
        before, after = text.rsplit("<channel|>", 1)
        thinking = before
        # Remove the opening tag + "thought\n" prefix if present
        if "<|channel>" in thinking:
            thinking = thinking.split("<|channel>", 1)[1]
            if thinking.startswith("thought\n"):
                thinking = thinking[len("thought\n"):]
            elif thinking.startswith("thought"):
                thinking = thinking[len("thought"):]
        return thinking.strip(), after.lstrip()

    return "", text


# ── System info ──────────────────────────────────────────────────────


def get_nobodywho_version() -> dict:
    """Get nobodywho version info: git commit if editable install, else package version."""
    result = {"version": "unknown", "git_commit": None}

    # Try to get package version first
    try:
        import importlib.metadata
        result["version"] = importlib.metadata.version("nobodywho")
    except Exception:
        pass

    # Try to get git commit if installed from local repo (editable install)
    try:
        pkg_path = os.path.dirname(nobodywho.__file__)
        # Walk up to find .git directory
        repo_path = pkg_path
        while repo_path != "/" and not os.path.exists(os.path.join(repo_path, ".git")):
            repo_path = os.path.dirname(repo_path)

        if os.path.exists(os.path.join(repo_path, ".git")):
            commit = subprocess.run(
                ["git", "rev-parse", "HEAD"],
                cwd=repo_path,
                capture_output=True,
                text=True,
                timeout=5,
            ).stdout.strip()
            if commit:
                result["git_commit"] = commit

            # Also check if there are uncommitted changes
            status = subprocess.run(
                ["git", "status", "--porcelain"],
                cwd=repo_path,
                capture_output=True,
                text=True,
                timeout=5,
            ).stdout.strip()
            if status:
                result["git_dirty"] = True
    except Exception:
        pass

    return result


def get_gpu_info() -> str:
    """Get GPU info via lspci, with sysfs fallback for NixOS"""
    # Try lspci first
    try:
        result = subprocess.run(["lspci"], capture_output=True, text=True, timeout=5)
        for line in result.stdout.split("\n"):
            if "VGA" in line or "3D controller" in line:
                return line.split(": ", 1)[-1]
    except Exception:
        pass

    # Fallback to sysfs (works without lspci on NixOS)
    try:
        import glob

        for card_path in glob.glob("/sys/class/drm/card*/device/uevent"):
            with open(card_path) as f:
                uevent = f.read()
            driver = None
            pci_id = None
            for line in uevent.strip().split("\n"):
                if line.startswith("DRIVER="):
                    driver = line.split("=")[1]
                elif line.startswith("PCI_ID="):
                    pci_id = line.split("=")[1]
            if driver and pci_id:
                return f"{driver} ({pci_id})"
    except Exception:
        pass

    return "unknown"


def get_cpu_model() -> str:
    """Get CPU model name, with fallback for Linux systems."""
    # Try platform.processor() first
    cpu = platform.processor()
    if cpu:
        return cpu
    # On Linux, read from /proc/cpuinfo
    try:
        with open("/proc/cpuinfo") as f:
            for line in f:
                if "model name" in line:
                    return line.split(":")[1].strip()
    except Exception:
        pass
    return "unknown"


def get_system_info() -> dict:
    """Gather system information for logging"""
    mem = psutil.virtual_memory()
    nobodywho_info = get_nobodywho_version()
    return {
        "cpu_model": get_cpu_model(),
        "cpu_count": psutil.cpu_count(logical=False),
        "memory_total_gb": round(mem.total / (1024**3), 2),
        "gpu_device": get_gpu_info(),
        "os": platform.system(),
        "nobodywho_version": nobodywho_info["version"],
        "nobodywho_commit": nobodywho_info.get("git_commit", ""),
        "nobodywho_dirty": nobodywho_info.get("git_dirty", False),
    }


# ── Output cleanup ───────────────────────────────────────────────────


# ── Model ────────────────────────────────────────────────────────────


@register_model("nobodywho")
class NobodyWhoLM(LM):
    MULTIMODAL = False

    chat: nobodywho.Chat
    allow_thinking: bool
    model_path: Path
    image_model_path: Path | None
    system_prompt: str | None
    response_processor: "Callable[[str], str] | None"
    stop_sequences_override: "list[str] | None"
    failed_samples: list[dict]
    total_samples: int
    total_tokens_generated: int
    total_generation_time: float
    sampler_config: dict
    _temp_dir: str | None

    def __init__(
        self,
        model_path: str,
        allow_thinking: str,
        n_ctx: int,
        system_prompt: str | None = None,
        image_model_path: str | None = None,
        *args,
        **kwargs,
    ):
        super().__init__()

        # model path
        assert isinstance(model_path, str)
        self.model_path = Path(model_path)
        assert self.model_path.exists()

        # allow thinking
        self.allow_thinking = True if allow_thinking.lower() == "true" else False

        # n_ctx
        assert n_ctx > 0
        assert isinstance(n_ctx, int)
        self.n_ctx = n_ctx

        # system prompt
        self.system_prompt = system_prompt

        # response processor (optional task-specific post-processing)
        self.response_processor = None
        # stop sequences override (None = use task defaults, [] = disable)
        self.stop_sequences_override = None

        # image model path (mmproj)
        if image_model_path is not None:
            self.image_model_path = Path(image_model_path)
            assert self.image_model_path.exists()
            self.MULTIMODAL = True
        else:
            self.image_model_path = None

        self._temp_dir = None
        self.failed_samples = []
        self.total_samples = 0
        self.total_tokens_generated = 0
        self.total_generation_time = 0.0
        self._init_chat()

    def __del__(self):
        if self._temp_dir and os.path.exists(self._temp_dir):
            shutil.rmtree(self._temp_dir, ignore_errors=True)

    def _init_chat(self):
        # Sampler config
        sampler = (
            nobodywho.SamplerBuilder()
            .temperature(0.6)
            .min_p(0.0, min_keep=1)
            .top_p(0.95, min_keep=1)
            .top_k(20)
            .dist()
        )
        self.sampler_config = {
            "temperature": 0.6,
            "min_p": 0.0,
            "top_p": 0.95,
            "top_k": 20,
        }
        kwargs = {
            "allow_thinking": self.allow_thinking,
            "n_ctx": self.n_ctx,
            "sampler": sampler,
        }
        if self.system_prompt is not None:
            kwargs["system_prompt"] = self.system_prompt
        if self.image_model_path is not None:
            model = nobodywho.Model(self.model_path, image_model_path=self.image_model_path)
            self.chat = nobodywho.Chat(model, **kwargs)
        else:
            self.chat = nobodywho.Chat(self.model_path, **kwargs)

    def _pil_to_path(self, pil_image) -> str:
        """Save a PIL image to a temp file and return its path."""
        if self._temp_dir is None:
            self._temp_dir = tempfile.mkdtemp(prefix="nw_eval_")
        fd, path = tempfile.mkstemp(suffix=".png", dir=self._temp_dir)
        os.close(fd)
        pil_image.save(path, format="PNG")
        return path

    def _build_multimodal_prompt(self, text: str, images: list) -> nobodywho.Prompt:
        """Split text on <image> placeholders and interleave with Image objects.

        Only as many images are used as there are <image> placeholders in the text,
        since some tasks provide more images in the visual list than are referenced.
        """
        segments = text.split("<image>")
        n_placeholders = len(segments) - 1
        n_images = min(n_placeholders, len(images))
        logger.debug(
            f"_build_multimodal_prompt: {n_placeholders} <image> placeholders, "
            f"{len(images)} images provided. "
            f"Text snippet: {text[:200]!r}"
        )

        parts = []
        for i, segment in enumerate(segments):
            if segment:
                parts.append(nobodywho.Text(segment))
            if i < n_images:
                path = self._pil_to_path(images[i])
                parts.append(nobodywho.Image(path))
        return nobodywho.Prompt(parts)

    def generate_until(self, requests: list[Instance], disable_tqdm=False):
        result: list[str] = []
        for request in tqdm([req.args for req in requests], disable=disable_tqdm):
            self.chat.reset_history()
            text = request[0]
            assert isinstance(text, str)

            # extract generation args (stop sequences, max tokens)
            request_args = request[1] if len(request) > 1 else {}
            max_gen_toks = request_args.get("max_gen_toks")  # None if not specified
            if self.stop_sequences_override is not None:
                until = self.stop_sequences_override
            else:
                until = request_args.get("until", [])

            # check for multimodal content (3rd element with "visual" key)
            images = None
            if len(request) >= 3 and isinstance(request[2], dict):
                images = request[2].get("visual")

            # calculate how many tokens to check for stop sequences
            # (each token is at least 1 char, so we need at most max_stop_len tokens)
            max_stop_len = max((len(s) for s in until), default=0)

            try:
                if images:
                    prompt = self._build_multimodal_prompt(text, images)
                    response_stream = self.chat.ask(prompt)
                else:
                    response_stream = self.chat.ask(text)

                # collect tokens, checking stop conditions
                tokens: list[str] = []

                # Track thinking state - only enforce limits after think block ends
                # If thinking is disabled, enforce limits from the start
                think_ended = not self.allow_thinking
                response_tokens = 0
                # Index where post-think tokens start (for stop sequence checking)
                think_end_idx = 0

                # Timing starts from first token (excludes prompt processing)
                gen_start_time: float | None = None

                for token in response_stream:
                    if gen_start_time is None:
                        gen_start_time = time.perf_counter()
                    tokens.append(token)

                    # !! HARD LIMIT: 16384 tokens max to prevent context exhaustion !!
                    if len(tokens) >= 16384:
                        self.chat.stop_generation()
                        break

                    # Detect when thinking block ends (check last 5 chunks only)
                    if self.allow_thinking and not think_ended:
                        recent = "".join(tokens[-5:])
                        if any(marker in recent for marker in THINK_END_MARKERS):
                            think_ended = True
                            response_tokens = 0
                            think_end_idx = len(tokens)

                    # Only enforce limits after think block
                    if think_ended:
                        response_tokens += 1

                        # check max token limit (only if specified by task)
                        if max_gen_toks is not None and response_tokens >= max_gen_toks:
                            self.chat.stop_generation()
                            break

                        # check stop sequences in post-think tokens only,
                        # stripping leading whitespace so newlines between
                        # </think> and the response don't trigger stops
                        post_think_text = "".join(tokens[think_end_idx:]).lstrip()
                        if until and post_think_text and any(stop_seq in post_think_text[-max_stop_len:] for stop_seq in until):
                            self.chat.stop_generation()
                            break

                # Record throughput stats (only if >= 2 tokens, counting N-1 intervals)
                if gen_start_time is not None and len(tokens) >= 2:
                    gen_elapsed = time.perf_counter() - gen_start_time
                    # We measure time from first token, so we have N-1 token intervals
                    self.total_tokens_generated += len(tokens) - 1
                    self.total_generation_time += gen_elapsed

                # Get completed text and extract response part (strip think block)
                full_response = response_stream.completed()
                if self.allow_thinking:
                    logger.debug(
                        f"Full response ({len(tokens)} tokens): "
                        f"{full_response[:200]!r}...{full_response[-200:]!r}"
                    )
                    print(f"  [DEBUG] completed() ({len(tokens)} tok): {full_response[:300]!r}")
                if self.allow_thinking:
                    thinking, response_text = strip_think_block(full_response)
                    if thinking:
                        print(f"  [DEBUG] after think-strip: {response_text[:300]!r}")
                    else:
                        print(f"  [DEBUG] no think block found, using full response")
                else:
                    response_text = full_response

                # truncate at stop sequence if present
                for stop_seq in until:
                    if stop_seq in response_text:
                        response_text = response_text.split(stop_seq)[0]
                        break

            except RuntimeError as e:
                logger.error(f"Generation failed: {e}")
                self.failed_samples.append({
                    "prompt": text[:500],  # truncate for logging
                    "error": str(e),
                })
                result.append("")
                self.total_samples += 1
                continue

            # Apply task-specific post-processing if configured
            if self.response_processor is not None:
                pre_processor = response_text
                response_text = self.response_processor(response_text)
                if self.allow_thinking and response_text != pre_processor:
                    print(f"  [DEBUG] after response_processor: {response_text[:300]!r}")

            final = response_text.strip()
            if self.allow_thinking:
                print(f"  [DEBUG] final result: {final[:300]!r}")

            result.append(final)
            self.total_samples += 1
        return result

    def get_model_info(self):
        """We're using this method to add additional metrics
        This is contingent on get_model_info being called *after* running the evals
        """
        # Get model file size in GB
        model_size_gb = round(self.model_path.stat().st_size / (1024**3), 2)

        # Calculate failure stats
        failed_count = len(self.failed_samples)
        failure_rate = failed_count / self.total_samples if self.total_samples > 0 else 0

        # Calculate throughput (tokens per second)
        tokens_per_second = (
            self.total_tokens_generated / self.total_generation_time
            if self.total_generation_time > 0
            else 0.0
        )

        return {
            "model_size_gb": model_size_gb,
            "failed_sample_count": failed_count,
            "failure_rate": round(failure_rate, 4),
            "total_tokens_generated": self.total_tokens_generated,
            "total_generation_time_sec": round(self.total_generation_time, 2),
            "tokens_per_second": round(tokens_per_second, 2),
            **get_system_info(),
        }

    def loglikelihood(self, *args, **kwargs):
        raise NotImplementedError

    def loglikelihood_rolling(self, *args, **kwargs):
        raise NotImplementedError

# ── CSV logging ──────────────────────────────────────────────────────

# All known metric columns per task (as returned by lm-eval)
TASK_METRICS: dict[str, list[str]] = {
    "ifeval": [
        "prompt_level_strict_acc,none",
        "inst_level_strict_acc,none",
        "prompt_level_loose_acc,none",
        "inst_level_loose_acc,none",
    ],
    "gsm8k": [
        "exact_match,strict-match",
        "exact_match,flexible-extract",
    ],
    "truthfulqa_gen": [
        "bleu_max,none",
        "bleu_acc,none",
        "bleu_diff,none",
    ],
    "humaneval": [
        "pass@1,create_test",
    ],
    "mbpp": [
        "pass_at_1,none",
    ],
    "drop": [
        "f1,none",
        "em,none",
    ],
    "mmmu_val_science": [
        "acc,none",
    ],
    "mmmu_val_humanities_and_social_science": [
        "acc,none",
    ],
}


def sanitize_metric_name(metric: str) -> str:
    """Convert lm-eval metric name to valid CSV column name.

    - Replace @ with _at_
    - Drop ',none' filter suffix (it's the default)
    - Replace ',' with '__' for other filters
    """
    metric = metric.replace("@", "_at_")
    if metric.endswith(",none"):
        return metric[:-5]  # drop ',none'
    return metric.replace(",", "__")


def get_all_metric_columns() -> list[str]:
    """Return all metric column names in consistent order, prefixed by task."""
    columns = []
    for task, metrics in TASK_METRICS.items():
        for metric in metrics:
            col_name = f"{task}_{sanitize_metric_name(metric)}"
            columns.append(col_name)
    return columns


def build_run_row(
    model_path: Path,
    task_results: dict[str, dict],  # task_name -> results dict from lm-eval
    limit: int | None,
    seed: int,
    total_duration: float,
    system_info: dict,
    failed_count: int = 0,
    total_samples: int = 0,
    total_tokens_generated: int = 0,
    total_generation_time: float = 0.0,
    sampler_config: dict | None = None,
    allow_thinking: bool = False,
) -> dict:
    """Build a flat dict for one CSV row from a complete run."""
    import time

    model_size_gb = round(model_path.stat().st_size / (1024**3), 2)
    failure_rate = failed_count / total_samples if total_samples > 0 else 0.0
    tokens_per_second = (
        total_tokens_generated / total_generation_time
        if total_generation_time > 0
        else 0.0
    )

    resolved_model_name = model_path.stem

    row = {
        "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
        "model_path": str(model_path),
        "model_name": resolved_model_name,
        "model_size_gb": model_size_gb,
        "limit": limit if limit is not None else "",
        "seed": seed,
        "duration_seconds": round(total_duration, 2),
        "total_samples": total_samples,
        "failed_samples": failed_count,
        "failure_rate": round(failure_rate, 4),
        "total_tokens_generated": total_tokens_generated,
        "generation_time_seconds": round(total_generation_time, 2),
        "tokens_per_second": round(tokens_per_second, 2),
        "sampler_config": str(sampler_config) if sampler_config else "",
        "allow_thinking": allow_thinking,
    }

    # Initialize all metric columns with empty string (not run)
    for col in get_all_metric_columns():
        row[col] = ""

    # Fill in metrics from completed tasks
    for task_name, task_data in task_results.items():
        metrics = task_data.get("results", {}).get(task_name, {})
        for metric_name, value in metrics.items():
            if metric_name.endswith(",stderr"):
                continue
            col_name = f"{task_name}_{sanitize_metric_name(metric_name)}"
            if col_name in row:
                row[col_name] = value

    # Add system info at the end
    row.update(system_info)

    return row


def get_csv_fieldnames(system_info_keys: list[str]) -> list[str]:
    """Return ordered list of all CSV column names."""
    base = [
        "timestamp", "model_path", "model_name", "model_size_gb",
        "limit", "seed", "duration_seconds",
        "total_samples", "failed_samples", "failure_rate",
        "total_tokens_generated", "generation_time_seconds", "tokens_per_second",
        "sampler_config",
        "allow_thinking",
    ]
    metrics = get_all_metric_columns()
    return base + metrics + system_info_keys


def append_run_to_csv(csv_path: Path, row: dict, system_info_keys: list[str]):
    """Append a run row to CSV, creating file with headers if needed."""
    import csv

    file_exists = csv_path.exists()

    if file_exists:
        # Read existing header so we write in the same column order
        with open(csv_path, newline="") as f:
            reader = csv.reader(f)
            fieldnames = next(reader)
    else:
        fieldnames = get_csv_fieldnames(system_info_keys)

    with open(csv_path, "a", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames, extrasaction="ignore")
        if not file_exists:
            writer.writeheader()
        writer.writerow(row)


# ── Output helpers ───────────────────────────────────────────────────


def print_samples(results: dict, max_samples: int = 5):
    """Print sample prompts and responses for debugging."""
    if "samples" not in results:
        print("No samples found in results (was log_samples=True?)")
        return

    for task_name, samples in results["samples"].items():
        print(f"\n{'='*60}")
        print(f"TASK: {task_name}")
        print(f"{'='*60}")

        for i, sample in enumerate(samples[:max_samples]):
            print(f"\n--- Sample {i+1} ---")

            # Print the prompt (doc)
            if "doc" in sample:
                doc = sample["doc"]
                if isinstance(doc, dict):
                    # Print relevant fields from the doc
                    for key in ["question", "prompt", "passage", "text"]:
                        if key in doc:
                            content = doc[key]
                            if len(content) > 500:
                                print(f"INPUT ({key}):\n{content[:500]}...")
                            else:
                                print(f"INPUT ({key}):\n{content}")
                            break
                else:
                    print(f"INPUT:\n{str(doc)[:500]}...")

            # Print the model's response
            if "resps" in sample:
                resps = sample["resps"]
                if resps and len(resps) > 0:
                    resp = resps[0]  # first response
                    if isinstance(resp, (list, tuple)) and len(resp) > 0:
                        resp = resp[0]
                    print(f"\nMODEL OUTPUT:\n{str(resp)[:1000]}")

            # Print the expected answer - try multiple sources
            if "doc" in sample and isinstance(sample["doc"], dict):
                doc = sample["doc"]
                # Task-specific answer extraction
                if "answers" in doc:
                    # DROP format: list of answer tuples
                    print(f"\nEXPECTED ANSWERS: {doc['answers']}")
                elif "answer" in doc:
                    # GSM8K and others
                    print(f"\nEXPECTED ANSWER: {doc['answer']}")
                elif "correct_answers" in doc:
                    # TruthfulQA format
                    print(f"\nCORRECT ANSWERS: {doc['correct_answers'][:3]}...")

            # Also show raw target if different
            if "target" in sample:
                target = str(sample["target"])
                if len(target) < 200:  # Only show if concise
                    print(f"TARGET: {target}")

            # Print filtered result if available
            if "filtered_resps" in sample:
                print(f"\nFILTERED: {sample['filtered_resps']}")

            print()


def print_results(results: dict):
    for task_name, metrics in results["results"].items():
        print(f"\n{task_name}:")
        for metric_name, value in metrics.items():
            if not metric_name.endswith(",stderr"):
                print(f"  {metric_name}: {value}")


# ── Incorrect sample collection ──────────────────────────────────────

# The per-sample metric key used to determine correctness per task.
# These are the raw keys from process_results(), WITHOUT filter suffixes.
# A sample is "incorrect" when this metric is falsy (0, 0.0, False).
PRIMARY_METRIC: dict[str, str] = {
    "ifeval": "prompt_level_strict_acc",
    "gsm8k": "exact_match",
    "truthfulqa_gen": "bleu_acc",
    "humaneval": "pass@1",
    "mbpp": "pass_at_1",
    "drop": "em",
    "mmmu_val_science": "acc",
    "mmmu_val_humanities_and_social_science": "acc",
}


def save_incorrect_samples(
    results: dict,
    task_name: str,
    output_path: Path,
    model_name: str,
    allow_thinking: bool,
):
    """Write incorrect samples to a JSONL file for debugging.

    Each line is a JSON object with prompt, response, thinking block,
    target, and per-sample metrics.
    """
    if "samples" not in results or task_name not in results["samples"]:
        logger.warning(f"No samples found for {task_name}, skipping incorrect sample export")
        return

    metric_key = PRIMARY_METRIC.get(task_name)
    if metric_key is None:
        logger.warning(f"No primary metric defined for {task_name}, skipping incorrect sample export")
        return

    samples = results["samples"][task_name]
    if not samples:
        return

    # Verify the metric key exists in the first sample
    first_sample_metrics = samples[0].get("metrics", [])
    if metric_key not in first_sample_metrics:
        logger.warning(
            f"Primary metric '{metric_key}' not found in {task_name} samples. "
            f"Available per-sample metrics: {first_sample_metrics}"
        )
        return

    incorrect = []

    for sample in samples:
        score = sample.get(metric_key)
        if score is None:
            continue
        # Falsy score = incorrect (covers 0, 0.0, False)
        if score:
            continue

        # Extract the full raw response
        full_response = ""
        if "resps" in sample and sample["resps"]:
            resp = sample["resps"][0]
            if isinstance(resp, (list, tuple)) and resp:
                full_response = str(resp[0])
            else:
                full_response = str(resp)

        # Split thinking from response
        thinking = ""
        response_text = full_response
        if allow_thinking:
            thinking, response_text = strip_think_block(full_response)

        # Extract prompt text from arguments
        prompt = ""
        if "arguments" in sample and sample["arguments"]:
            arg = sample["arguments"][0]
            if isinstance(arg, (list, tuple)) and arg:
                prompt = str(arg[0])
            else:
                prompt = str(arg)

        # Collect all per-sample metric values
        metric_names = sample.get("metrics", [])
        metrics = {}
        for m in metric_names:
            if m in sample:
                val = sample[m]
                # Convert non-serializable types
                if isinstance(val, (list, tuple)):
                    val = [bool(v) if isinstance(v, bool) else v for v in val]
                metrics[m] = val

        record = {
            "task": task_name,
            "model": model_name,
            "doc_id": sample.get("doc_id"),
            "prompt": prompt,
            "response": response_text,
            "thinking": thinking,
            "full_response": full_response,
            "target": sample.get("target", ""),
            "metrics": metrics,
        }
        incorrect.append(record)

    if not incorrect:
        print(f"  No incorrect samples for {task_name}")
        return

    with open(output_path, "a") as f:
        for record in incorrect:
            f.write(json.dumps(record, ensure_ascii=False) + "\n")

    print(f"  Saved {len(incorrect)} incorrect samples to {output_path}")
