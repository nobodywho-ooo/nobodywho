import argparse
import logging
import os
import platform
import random
import subprocess
from pathlib import Path

import lm_eval
import mlflow
import nobodywho
import psutil
from lm_eval.api.instance import Instance
from lm_eval.api.model import LM
from lm_eval.api.registry import register_model
from lm_eval.loggers import EvaluationTracker, WandbLogger
from tqdm import tqdm

logger = logging.getLogger(__name__)


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


@register_model("nobodywho")
class NobodyWhoLM(LM):
    chat: nobodywho.Chat
    allow_thinking: bool
    model_path: Path
    system_prompt: str | None
    failed_samples: list[dict]
    max_retries: int
    total_samples: int

    def __init__(
        self,
        model_path: str,
        allow_thinking: str,
        n_ctx: int,
        system_prompt: str | None = None,
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

        self.failed_samples = []
        self.max_retries = 2
        self.total_samples = 0
        self._init_chat()

    def _init_chat(self):
        # Custom sampler: Temperature=0.7, TopP=0.8, TopK=20, MinP=0
        sampler = (
            nobodywho.SamplerBuilder()
            .top_k(20)
            .top_p(0.8, min_keep=1)
            .min_p(0.0, min_keep=1)
            .temperature(0.7)
            .dist()
        )
        kwargs = {
            "allow_thinking": self.allow_thinking,
            "n_ctx": self.n_ctx,
            "sampler": sampler,
        }
        if self.system_prompt is not None:
            kwargs["system_prompt"] = self.system_prompt
        self.chat = nobodywho.Chat(self.model_path, **kwargs)

    def generate_until(self, requests: list[Instance], disable_tqdm=False):
        result: list[str] = []
        for request in tqdm([req.args for req in requests], disable=disable_tqdm):
            self.chat.reset_history()
            text = request[0]
            assert isinstance(text, str)

            # extract generation args (stop sequences, max tokens)
            request_args = request[1] if len(request) > 1 else {}
            max_gen_toks = request_args.get("max_gen_toks")  # None if not specified
            until = request_args.get("until", [])

            # calculate how many chunks to check for stop sequences
            # (each token is at least 1 char, so we need at least max_stop_len chunks)
            max_stop_len = max((len(s) for s in until), default=0)

            # do the generation with retry logic
            response_text: str | None = None
            last_error: Exception | None = None

            for attempt in range(self.max_retries):
                try:
                    response_stream = self.chat.ask(text)

                    # collect chunks, checking stop conditions
                    chunks: list[str] = []

                    # Track thinking state - only enforce limits after think block ends
                    # If thinking is disabled, enforce limits from the start
                    think_ended = not self.allow_thinking
                    response_tokens = 0

                    for chunk in response_stream:
                        chunks.append(chunk)

                        # Detect when thinking block ends (check last 5 chunks only)
                        if self.allow_thinking and not think_ended:
                            recent = "".join(chunks[-5:])
                            if "</think>" in recent:
                                think_ended = True
                                response_tokens = 0

                        # Only enforce limits after think block
                        if think_ended:
                            response_tokens += 1

                            # check max token limit (only if specified by task)
                            if max_gen_toks is not None and response_tokens >= max_gen_toks:
                                self.chat.stop_generation()
                                break

                            # check stop sequences in recent chunks
                            if until:
                                recent_text = "".join(chunks[-max_stop_len:])
                                if any(stop_seq in recent_text for stop_seq in until):
                                    self.chat.stop_generation()
                                    break

                    # Get completed text and extract response part (strip think block)
                    full_response = response_stream.completed()
                    if self.allow_thinking and "</think>" in full_response:
                        response_text = full_response.split("</think>", 1)[1]
                    else:
                        response_text = full_response

                    # truncate at stop sequence if present
                    for stop_seq in until:
                        if stop_seq in response_text:
                            response_text = response_text.split(stop_seq)[0]
                            break

                    break  # success, exit retry loop

                except RuntimeError as e:
                    last_error = e
                    logger.warning(
                        f"Generation attempt {attempt + 1}/{self.max_retries} failed: {e}"
                    )
                    self._init_chat()

            # if all retries failed, track the failure and return empty string
            if response_text is None:
                logger.error(
                    f"All {self.max_retries} generation attempts failed: {last_error}"
                )
                self.failed_samples.append({
                    "prompt": text[:500],  # truncate for logging
                    "error": str(last_error),
                })
                result.append("")
                self.total_samples += 1
                continue

            # strip markdown code block markers if present
            # (fixes MBPP code extraction which expects raw code)
            response_text = response_text.strip()
            if response_text.startswith("```"):
                # remove the opening ``` and optional language identifier
                lines = response_text.split("\n", 1)
                if len(lines) > 1:
                    response_text = lines[1]  # skip the ```python line
                else:
                    response_text = response_text[3:]  # just remove ```
            # also strip trailing ```
            if response_text.rstrip().endswith("```"):
                response_text = response_text.rstrip()[:-3].rstrip()

            result.append(response_text)
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

        return {
            "model_size_gb": model_size_gb,
            "failed_sample_count": failed_count,
            "failure_rate": round(failure_rate, 4),
            **get_system_info(),
        }

    def loglikelihood(self, *args, **kwargs):
        raise NotImplementedError

    def loglikelihood_rolling(self, *args, **kwargs):
        raise NotImplementedError


DEFAULT_TASKS = [
    "ifeval",  # instruction following: mostly text formatting tasks
    "gsm8k",  # high-school level math reasoning problems
    "truthfulqa_gen",  # facts!
    # python coding
    "humaneval",  # this one requires evaluation of machine-generated python code
    "mbpp",  # more pythoncode
    # Reading comprehension (context provided):
    "drop",  # ✅ Reading + arithmetic reasoning
    # "race",  # ✅ Reading comprehension - requires loglikelihood
    # Commonsense reasoning:
    # "piqa",  # ✅ Physical commonsense - requires loglikelihood
    # "winogrande",  # ✅ Pronoun resolution (reasoning) - requires loglikelihood
    # "arc_challenge",  # ✅ Science reasoning (not pure facts) - requires loglikelihood
    # these are all the mmlu tasks that are "elementary" and "high school" level
    # "mmlu_elementary_mathematics",
    # "mmlu_high_school_biology",
    # "mmlu_high_school_chemistry",
    # "mmlu_high_school_computer_science",
    # "mmlu_high_school_european_history",
    # "mmlu_high_school_geography",
    # "mmlu_high_school_government_and_politics",
    # "mmlu_high_school_macroeconomics",
    # "mmlu_high_school_mathematics",
    # "mmlu_high_school_microeconomics",
    # "mmlu_high_school_physics",
    # "mmlu_high_school_psychology",
    # "mmlu_high_school_statistics",
    # "mmlu_high_school_us_history",
    # "mmlu_high_school_world_history",
    # "bbh", # these are huge and have a lot of subtasks.
    # "mmlu_generative", # these are huge and have a lot of subtasks.
    # "truthfulqa_gen",
    #  "hellaswag" # this requires loglikelyhood data
]


def make_hf_tracker(hf_token) -> EvaluationTracker:
    """simple_evaluate handles saving/uploading when this tracker is passed to it."""
    print("Making HF Tracker...")
    return EvaluationTracker(
        output_path="./eval-results",
        push_results_to_hub=True,
        push_samples_to_hub=True,
        hub_results_org="NobodyWho",
        details_repo_name="eval-results",
        results_repo_name="eval-results",
        public_repo=False,
        token=hf_token,
    )


def make_wandb_logger(run_name: str, model_path: Path, tasks: list[str]) -> WandbLogger:
    print("Making WandbLogger...")
    return WandbLogger(
        init_args={
            "project": "nobodywho-evals",
            "name": run_name,
            "tags": ["nobodywho", "eval"],
        },
        config_args={"model_path": str(model_path), "tasks": tasks},
    )


def make_mlflow_run(
    run_name: str,
    model_path: Path,
    tracking_uri: str,
    experiment_name: str,
    tasks: list[str],
):
    print("Making MLFlow run...")
    mlflow.set_tracking_uri(tracking_uri)
    mlflow.set_experiment(experiment_name)
    run = mlflow.start_run(run_name=run_name)

    # Log critical identifying info early (survives crashes)
    mlflow.log_param("model_path_input", str(model_path))
    mlflow.log_param("tasks_input", ",".join(tasks))

    # Set tag for model name (may populate "model" column in MLflow UI)
    mlflow.set_tag("model_name", model_path.name)

    return run


def log_to_wandb(logger: WandbLogger, results: dict):
    logger.post_init(results)
    logger.log_eval_result()
    if samples := results.get("samples"):
        try:
            logger.log_eval_samples(samples)
        except Exception as e:
            print(f"Warning: Failed to log samples to W&B: {e}")
    logger.run.finish()


def sanitize_metric_name(name: str) -> str:
    """Sanitize metric name for MLflow (no commas allowed)."""
    # Replace commas with underscores, strip ',none' suffix
    # MLflow only allows: alphanumerics, underscores, dashes, periods, spaces, colons, slashes
    name = name.replace(",none", "").replace(",", "_").replace("@", "_at_")
    return name


def log_to_mlflow(
    results: dict,
    system_prompt: str | None = None,
    sampler_config: str | None = None,
):
    # Log per-task metrics
    for task_name, metrics in results["results"].items():
        for metric_name, value in metrics.items():
            if isinstance(value, (int, float)):
                clean_name = sanitize_metric_name(f"{task_name}/{metric_name}")
                mlflow.log_metric(clean_name, value)

    # Log system prompt
    if system_prompt is not None:
        # Truncate if too long for param (MLflow has 500 char limit for params)
        if len(system_prompt) <= 500:
            mlflow.log_param("system_prompt", system_prompt)
        else:
            mlflow.log_param("system_prompt", system_prompt[:497] + "...")
            # Log full prompt as artifact
            mlflow.log_text(system_prompt, "system_prompt.txt")
    else:
        mlflow.log_param("system_prompt", "")

    # Log sampler config
    if sampler_config:
        mlflow.log_param("sampler_config", sampler_config)

    # Log model/system metrics from config (includes get_model_info data)
    if "config" in results:
        for key, value in results["config"].items():
            if isinstance(value, bool):
                mlflow.log_param(key, str(value))
            elif isinstance(value, (int, float)):
                mlflow.log_metric(f"model/{key}", value)
            elif isinstance(value, str):
                # Special handling for "model" field - log as both param and tag
                if key == "model":
                    mlflow.set_tag("model", value)
                mlflow.log_param(key, value)
            elif isinstance(value, dict) and key == "model_args":
                # Flatten and log model_args
                for arg_name, arg_value in value.items():
                    mlflow.log_param(f"model_args.{arg_name}", arg_value)

    # Log environment info (added by lm_eval's add_env_info)
    env_fields = ["pretty_env_info", "lm_eval_version", "git_hash"]
    for field in env_fields:
        if field in results and results[field]:
            mlflow.log_param(f"env.{field}", str(results[field]))

    mlflow.end_run()


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


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Run lm-eval benchmarks with nobodywho inference backend"
    )
    parser.add_argument(
        "--model", "-m",
        type=str,
        default=os.getenv("TEST_MODEL"),
        help="Path to GGUF model file (or set TEST_MODEL env var)",
    )
    parser.add_argument(
        "--tasks", "-t",
        type=str,
        default=None,
        help=f"Comma-separated list of tasks (default: {','.join(DEFAULT_TASKS)})",
    )
    parser.add_argument(
        "--limit", "-l",
        type=int,
        default=None,
        help="Number of samples per task (default: no limit)",
    )
    parser.add_argument(
        "--print-samples",
        action="store_true",
        help="Print sample prompts and responses after evaluation",
    )
    parser.add_argument(
        "--shuffle",
        action="store_true",
        help="Randomly sample instead of taking first N (recommended for DROP)",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=42,
        help="Random seed for shuffled sampling (default: 42)",
    )
    parser.add_argument(
        "--system-prompt",
        type=str,
        default=None,
        help="System prompt to use for generation",
    )
    parser.add_argument(
        "--system-prompt-file",
        type=str,
        default=None,
        help="Path to file containing system prompt",
    )
    args = parser.parse_args()

    # Handle system prompt from file
    if args.system_prompt_file:
        if args.system_prompt:
            parser.error("Cannot use both --system-prompt and --system-prompt-file")
        with open(args.system_prompt_file) as f:
            args.system_prompt = f.read().strip()

    # allow code eval: this lets the model run code. yolo.
    os.environ["HF_ALLOW_CODE_EVAL"] = "1"

    # Configure logging
    logging.basicConfig(level=logging.WARNING)

    # Model path (required)
    assert args.model is not None, "Model path required: use --model or set TEST_MODEL"
    model_path = Path(args.model)
    assert model_path.exists(), f"Model not found: {model_path}"
    run_name = f"eval-{model_path.name}"

    # Tasks and limit
    run_tasks = args.tasks.split(",") if args.tasks else DEFAULT_TASKS
    limit = args.limit  # None means no limit

    print(f"Tasks: {', '.join(run_tasks)}")
    print(f"Limit: {limit if limit else 'none'}")
    if args.shuffle and limit:
        print(f"Shuffle: enabled (seed={args.seed})")

    tracker = make_hf_tracker(hf_token) if (hf_token := os.getenv("HF_TOKEN")) else None
    wandb_logger = (
        make_wandb_logger(run_name, model_path, run_tasks)
        if os.getenv("WANDB_API_KEY")
        else None
    )
    mlflow_run = (
        make_mlflow_run(
            run_name, model_path, mlflow_uri, os.environ["MLFLOW_EXPERIMENT_NAME"], run_tasks
        )
        if (mlflow_uri := os.getenv("MLFLOW_TRACKING_URI"))
        else None
    )

    if args.system_prompt:
        print(f"System prompt: {args.system_prompt}")

    print("Starting evals suite...")

    # Create model instance ourselves so we can access failure stats after
    model_instance = NobodyWhoLM(
        model_path=str(model_path.resolve()),
        allow_thinking="true",
        n_ctx=32768,
        system_prompt=args.system_prompt,
    )

    # Build samples dict for random sampling, or use limit for first-N
    samples_dict = None
    eval_limit = limit

    if args.shuffle and limit:
        # Get task objects to find dataset sizes
        from lm_eval.tasks import TaskManager

        random.seed(args.seed)
        tm = TaskManager()
        task_dict = tm.load_task_or_group(run_tasks)
        samples_dict = {}

        for task_name, task_obj in task_dict.items():
            dataset_size = len(task_obj.eval_docs)
            n_samples = min(limit, dataset_size)
            samples_dict[task_name] = random.sample(range(dataset_size), n_samples)
            print(f"  {task_name}: {n_samples}/{dataset_size} samples (random)")

        eval_limit = None  # Use samples instead of limit

    results = lm_eval.simple_evaluate(
        model=model_instance,
        confirm_run_unsafe_code=True,  # run ml-generated code
        tasks=run_tasks,
        log_samples=True,
        evaluation_tracker=tracker,
        limit=eval_limit,
        samples=samples_dict,
    )
    assert results is not None

    if wandb_logger:
        log_to_wandb(wandb_logger, results)
    if mlflow_run:
        log_to_mlflow(
            results,
            system_prompt=args.system_prompt,
            sampler_config="top_k=20, top_p=0.8, min_p=0.0, temperature=0.7, dist",
        )
    print_results(results)

    # Print sample outputs if requested
    if args.print_samples:
        print_samples(results, max_samples=limit or 5)

    # Print failure summary
    if model_instance.failed_samples:
        print(f"\n--- Generation Failures ({len(model_instance.failed_samples)} total) ---")
        for i, failure in enumerate(model_instance.failed_samples[:10]):  # show first 10
            print(f"\n[{i+1}] Error: {failure['error']}")
            print(f"    Prompt: {failure['prompt'][:100]}...")
        if len(model_instance.failed_samples) > 10:
            print(f"\n... and {len(model_instance.failed_samples) - 10} more failures")
