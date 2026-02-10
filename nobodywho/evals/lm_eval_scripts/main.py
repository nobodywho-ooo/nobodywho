import os

import lm_eval
import nobodywho
from lm_eval.api.instance import Instance
from lm_eval.api.model import LM
from lm_eval.api.registry import register_model
from lm_eval.loggers import EvaluationTracker, WandbLogger
from tqdm import tqdm


@register_model("nobodywho-test")
class NobodyWhoLM(LM):
    chat: nobodywho.Chat

    def __init__(self, *args, **kwargs):
        super().__init__()
        model_path = os.getenv("TEST_MODEL")
        assert isinstance(model_path, str)
        self.chat = nobodywho.Chat(model_path, allow_thinking=False, n_ctx=16384)

    def generate_until(self, requests: list[Instance], disable_tqdm=False):
        result: list[str] = []
        for request in tqdm([req.args for req in requests], disable=disable_tqdm):
            self.chat.reset_history()
            text = request[0]
            assert isinstance(text, str)

            # XXX: these provide additional generation args like stopwords or max_tokens
            # request_args = request[1]

            response_text = self.chat.ask(text).completed()
            result.append(response_text)
        return result

    def loglikelihood(self, *args, **kwargs):
        raise NotImplementedError

    def loglikelihood_rolling(self, *args, **kwargs):
        raise NotImplementedError


tasks = [
    "ifeval",
    "truthfulqa_gen",
    # "humaneval", # this one requires evaluation machine-generated python code
    #  "hellaswag" # this requires loglikelyhood data
]

if __name__ == "__main__":
    tracker = None
    if hf_token := os.getenv("HF_TOKEN"):
        # create HF tracker if token env var is present
        print("Creating HF eval tracker...")
        tracker = EvaluationTracker(
            output_path="./eval-results",  # Local save path
            push_results_to_hub=True,  # Enable Hub upload
            push_samples_to_hub=True,  # Upload detailed samples (requires log_samples=True)
            hub_results_org="NobodyWho",  # Your HF org (defaults to token owner)
            details_repo_name="eval-results",  # Repo for detailed results
            results_repo_name="eval-results",  # Repo for aggregated results (can be same)
            public_repo=False,  # Private repo
            token=hf_token,  # Uses HF_TOKEN env var if None
        )

    wandb_logger = None
    if os.getenv("WANDB_API_KEY") or os.getenv("WANDB_MODE") == "disabled":
        # Create W&B logger if API key is set or if explicitly enabled
        print("Creating W&B logger...")
        model_path = os.getenv("TEST_MODEL", "unknown")
        wandb_logger = WandbLogger(
            init_args={
                "project": "nobodywho-evals",
                "name": f"eval-{os.path.basename(model_path)}",
                "tags": ["nobodywho", "eval"],
            },
            config_args={
                "model_path": model_path,
                "tasks": tasks,
            },
        )

    print("Starting evals suite")
    results = lm_eval.simple_evaluate(
        model="nobodywho-test",
        model_args="",
        tasks=tasks,
        log_samples=True,
        evaluation_tracker=tracker,
    )

    if results:
        samples = results.get("samples", None)

        # HF Hub logging
        if tracker:
            print("Saving and uploading results to HF Hub...")
            results_copy = dict(results)  # Make a copy to avoid mutating
            samples_copy = results_copy.pop("samples", None)

            # Save and upload aggregated results
            tracker.save_results_aggregated(results=results_copy, samples=samples_copy)

            # Save and upload per-task samples (if you logged them)
            if samples_copy:
                for task_name in results_copy["configs"].keys():
                    tracker.save_results_samples(
                        task_name=task_name, samples=samples_copy[task_name]
                    )

            # Update the metadata card
            if tracker.push_results_to_hub or tracker.push_samples_to_hub:
                tracker.recreate_metadata_card()

        # W&B logging
        if wandb_logger:
            print("Logging results to W&B...")
            wandb_logger.post_init(results)
            wandb_logger.log_eval_result()

            # Log detailed samples (skip if it fails)
            if samples:
                try:
                    wandb_logger.log_eval_samples(samples)
                except Exception as e:
                    print(f"Warning: Failed to log samples to W&B: {e}")
                    print("Continuing without sample logging...")

            # Finish the W&B run
            wandb_logger.run.finish()
            print("W&B logging complete!")

    # Iterate through all tasks and print results
    for task_name, metrics in results["results"].items():
        print(f"\n{task_name}:")
        for metric_name, value in metrics.items():
            if not metric_name.endswith(",stderr"):  # Skip stderr for cleaner output
                print(f"  {metric_name}: {value}")
