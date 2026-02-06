import json
import os
from pathlib import Path

import ifeval
import typer
from nobodywho import Chat
from tqdm import tqdm

app = typer.Typer(help="Run IFEval benchmark on GGUF models")


def generate_responses(
    model_file: Path,
    dataset_file: Path,
    responses_file: Path,
    limit: int | None = None,
):
    chat = Chat(str(model_file))

    results = []
    lines = [line for line in open(dataset_file)]

    if limit is not None:
        lines = lines[:limit]
        typer.echo(f"Limiting to first {limit} tasks")

    for line in tqdm(lines, desc="Generating responses"):
        task = json.loads(line)

        chat.reset_history()
        response: str = chat.ask(task["prompt"]).completed()
        results.append({"prompt": task["prompt"], "response": response})

    jsonl_str = "\n".join(json.dumps(r) for r in results)
    with open(responses_file, "w") as f:
        f.write(jsonl_str)

    typer.echo(f"Responses saved to {responses_file}")


def evaluate_responses(
    dataset_file: Path,
    responses_file: Path,
):
    evaluator = ifeval.Evaluator(ifeval.instruction_registry)
    input_examples = ifeval.read_input_examples(str(dataset_file))
    responses = ifeval.read_responses(str(responses_file))
    report, all_outputs = evaluator.evaluate(input_examples, responses)

    typer.echo("\n=== Evaluation Results ===")
    typer.echo(f"Strict prompt accuracy: {report['eval_results_strict']['prompt_accuracy']}")
    typer.echo(f"Loose prompt accuracy: {report['eval_results_loose']['prompt_accuracy']}")


@app.command()
def generate(
    model_file: Path = typer.Option(
        None,
        "--model",
        "-m",
        help="Path to GGUF model file",
        envvar="TEST_MODEL",
    ),
    dataset_file: Path = typer.Option(
        "ifeval_input_data.jsonl",
        "--dataset-file",
        "-d",
        help="Path to input dataset JSONL file",
    ),
    responses_file: Path | None = typer.Option(
        None,
        "--responses-file",
        "-r",
        help="Path to output responses JSONL file (default: responses_{model_name}.jsonl)",
    ),
    limit: int | None = typer.Option(
        None,
        "--limit",
        "-n",
        help="Limit number of tasks to process (useful for quick testing)",
        min=1,
    ),
):
    """Generate responses from the model without evaluation."""
    if model_file is None:
        typer.echo("Error: Model file must be provided via --model or TEST_MODEL env var", err=True)
        raise typer.Exit(1)

    if not model_file.exists():
        typer.echo(f"Error: Model file not found: {model_file}", err=True)
        raise typer.Exit(1)

    if not dataset_file.exists():
        typer.echo(f"Error: Dataset file not found: {dataset_file}", err=True)
        raise typer.Exit(1)

    # Generate default responses filename based on model name if not provided
    if responses_file is None:
        model_name = model_file.stem  # Get filename without extension
        responses_file = Path(f"responses_{model_name}.jsonl")

    generate_responses(model_file, dataset_file, responses_file, limit)


@app.command()
def evaluate(
    dataset_file: Path = typer.Option(
        "ifeval_input_data.jsonl",
        "--dataset-file",
        "-d",
        help="Path to input dataset JSONL file",
    ),
    responses_file: Path = typer.Option(
        "responses.jsonl",
        "--responses-file",
        "-r",
        help="Path to responses JSONL file to evaluate",
    ),
):
    """Evaluate pre-generated responses."""
    if not dataset_file.exists():
        typer.echo(f"Error: Dataset file not found: {dataset_file}", err=True)
        raise typer.Exit(1)

    if not responses_file.exists():
        typer.echo(f"Error: Responses file not found: {responses_file}", err=True)
        raise typer.Exit(1)

    evaluate_responses(dataset_file, responses_file)


@app.command()
def run(
    model_file: Path = typer.Option(
        None,
        "--model",
        "-m",
        help="Path to GGUF model file",
        envvar="TEST_MODEL",
    ),
    dataset_file: Path = typer.Option(
        "ifeval_input_data.jsonl",
        "--dataset-file",
        "-d",
        help="Path to input dataset JSONL file",
    ),
    responses_file: Path | None = typer.Option(
        None,
        "--responses-file",
        "-r",
        help="Path to output responses JSONL file (default: responses_{model_name}.jsonl)",
    ),
    limit: int | None = typer.Option(
        None,
        "--limit",
        "-n",
        help="Limit number of tasks to process (useful for quick testing)",
        min=1,
    ),
):
    """Generate responses and evaluate them (default workflow)."""
    if model_file is None:
        typer.echo("Error: Model file must be provided via --model or TEST_MODEL env var", err=True)
        raise typer.Exit(1)

    if not model_file.exists():
        typer.echo(f"Error: Model file not found: {model_file}", err=True)
        raise typer.Exit(1)

    if not dataset_file.exists():
        typer.echo(f"Error: Dataset file not found: {dataset_file}", err=True)
        raise typer.Exit(1)

    # Generate default responses filename based on model name if not provided
    if responses_file is None:
        model_name = model_file.stem  # Get filename without extension
        responses_file = Path(f"responses_{model_name}.jsonl")

    generate_responses(model_file, dataset_file, responses_file, limit)
    evaluate_responses(dataset_file, responses_file)


if __name__ == "__main__":
    app()
