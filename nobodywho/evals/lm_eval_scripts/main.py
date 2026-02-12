import logging
import os
from pathlib import Path

import lm_eval
import mlflow
import nobodywho
from lm_eval.api.instance import Instance
from lm_eval.api.model import LM
from lm_eval.api.registry import register_model
from lm_eval.loggers import EvaluationTracker, WandbLogger
from tqdm import tqdm

logger = logging.getLogger(__name__)


@register_model("nobodywho-test")
class NobodyWhoLM(LM):
    chat: nobodywho.Chat

    def __init__(self, *args, **kwargs):
        super().__init__()
        model_path = os.getenv("TEST_MODEL")
        assert isinstance(model_path, str)
        self.chat = nobodywho.Chat(model_path, allow_thinking=True, n_ctx=16384 * 2)

    def generate_until(self, requests: list[Instance], disable_tqdm=False):
        result: list[str | None] = []
        for request in tqdm([req.args for req in requests], disable=disable_tqdm):
            self.chat.reset_history()
            text = request[0]
            assert isinstance(text, str)

            # these provide additional generation args like stopwords or max_tokens
            request_args = request[1]
            allow_thinking = request_args.get("allow_thinking", "true")
            assert isinstance(allow_thinking, str)
            allow_thinking = True if allow_thinking.lower() == "true" else False
            self.chat.set_allow_thinking(allow_thinking)

            # do the generation
            try:
                response_text = self.chat.ask(text).completed()
            except RuntimeError as e:
                logger.error(f"Exception during generation: {e}")
                result.append(None)
                continue

            # remove think block from response
            # XXX: this is model/token-specific can we do this in an agnostic way?
            #      it will require changes to nobodywho "upstream"
            if "</think>" in response_text:
                response_text = response_text.split("</think>")[1]

            result.append(response_text)
        return result

    def loglikelihood(self, *args, **kwargs):
        raise NotImplementedError

    def loglikelihood_rolling(self, *args, **kwargs):
        raise NotImplementedError


tasks = [
    "ifeval",
    # "truthfulqa_gen",
    # "humaneval", # this one requires evaluation machine-generated python code
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


def make_wandb_logger(run_name: str, model_path: Path) -> WandbLogger:
    print("Making WandbLogger...")
    return WandbLogger(
        init_args={
            "project": "nobodywho-evals",
            "name": run_name,
            "tags": ["nobodywho", "eval"],
        },
        config_args={"model_path": str(model_path), "tasks": tasks},
    )


def make_mlflow_run(run_name: str, model_path: Path, tracking_uri: str):
    print("Making MLFlow run...")
    mlflow.set_tracking_uri(tracking_uri)
    mlflow.set_experiment("nobodywho-evals")
    run = mlflow.start_run(run_name=run_name)
    mlflow.log_param("model_path", str(model_path))
    mlflow.log_param("tasks", ",".join(tasks))
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


def log_to_mlflow(results: dict):
    for task_name, metrics in results["results"].items():
        for metric_name, value in metrics.items():
            if isinstance(value, (int, float)):
                mlflow.log_metric(f"{task_name}/{metric_name}", value)
    mlflow.end_run()


def print_results(results: dict):
    for task_name, metrics in results["results"].items():
        print(f"\n{task_name}:")
        for metric_name, value in metrics.items():
            if not metric_name.endswith(",stderr"):
                print(f"  {metric_name}: {value}")


if __name__ == "__main__":
    model_path = os.getenv("TEST_MODEL")
    assert isinstance(model_path, str)
    model_path = Path(model_path)
    assert model_path.exists()
    run_name = f"eval-{model_path.name}"

    tracker = make_hf_tracker(hf_token) if (hf_token := os.getenv("HF_TOKEN")) else None
    wandb_logger = (
        make_wandb_logger(run_name, model_path) if os.getenv("WANDB_API_KEY") else None
    )
    mlflow_run = (
        make_mlflow_run(run_name, model_path, mlflow_uri)
        if (mlflow_uri := os.getenv("MLFLOW_TRACKING_URI"))
        else None
    )

    print("Starting evals suite...")
    results = lm_eval.simple_evaluate(
        model="nobodywho-test",
        model_args={"allow_thinking": "True"},
        tasks=tasks,
        log_samples=True,
        evaluation_tracker=tracker,
        limit=10,
    )
    assert results is not None

    if wandb_logger:
        log_to_wandb(wandb_logger, results)
    if mlflow_run:
        log_to_mlflow(results)
    print_results(results)
