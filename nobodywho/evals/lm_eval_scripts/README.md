# NobodyWho LM Evaluation Harness

This script runs the [lm-eval-harness](https://github.com/EleutherAI/lm-evaluation-harness) benchmarks using the `nobodywho` inference backend.

## Quick Start

```bash
# Run full eval suite
python main.py --model /path/to/model.gguf

# Run specific tasks with sample limit
python main.py --model /path/to/model.gguf --tasks gsm8k,mbpp --limit 100
```

## CLI Arguments

| Argument | Short | Description |
|----------|-------|-------------|
| `--model` | `-m` | Path to GGUF model file (or set `TEST_MODEL` env var) |
| `--tasks` | `-t` | Comma-separated list of tasks (default: all) |
| `--limit` | `-l` | Number of samples per task (default: no limit) |
| `--shuffle` | | Randomly sample instead of first N (recommended for DROP) |
| `--seed` | | Random seed for shuffled sampling (default: 42) |
| `--print-samples` | | Print prompts and responses after evaluation |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `TEST_MODEL` | Fallback for `--model` if not provided |
| `HF_TOKEN` | HuggingFace token for uploading results |
| `WANDB_API_KEY` | Weights & Biases API key for logging |
| `MLFLOW_TRACKING_URI` | MLflow server URI |
| `MLFLOW_EXPERIMENT_NAME` | MLflow experiment name (required if using MLflow) |

## Examples

```bash
# Run GSM8K with 50 samples and print outputs
python main.py -m ./model.gguf -t gsm8k -l 50 --print-samples

# Run code benchmarks only
python main.py -m ./model.gguf -t humaneval,mbpp

# Run full suite with 500 samples each
python main.py -m ./model.gguf -l 500

# Run DROP with random sampling (recommended - avoids passage grouping bias)
python main.py -m ./model.gguf -t drop -l 500 --shuffle

# Reproducible random sampling with custom seed
python main.py -m ./model.gguf -l 100 --shuffle --seed 123
```

### Why use `--shuffle`?

Some datasets like DROP group multiple questions per passage (~15 questions each). Without shuffling, `--limit 500` would only cover ~35 passages. With `--shuffle`, samples are spread across the full dataset for better coverage.

## Tasks

The default eval suite runs these benchmarks:

| Task | Type | Description |
|------|------|-------------|
| `ifeval` | Instruction following | Text formatting tasks |
| `gsm8k` | Math reasoning | High-school level math problems |
| `truthfulqa_gen` | Factual accuracy | Tests for common misconceptions |
| `humaneval` | Code generation | Python function completion |
| `mbpp` | Code generation | Python programming problems |
| `drop` | Reading comprehension | Reading + arithmetic reasoning |

## Model Adapter

The `NobodyWhoLM` class adapts `nobodywho.Chat` to the lm-eval interface:

- **Thinking support**: Limits (max tokens, stop sequences) only enforced after `</think>` block
- **Retry logic**: Retries failed generations up to 2 times
- **Stop sequences**: Uses `chat.stop_generation()` for early termination
- **Code block cleanup**: Removes markdown fences for code tasks
- **Failure tracking**: Logs failed samples with error details

## Logging Backends

Results can be logged to multiple backends simultaneously:

### MLflow
```bash
export MLFLOW_TRACKING_URI=http://localhost:5000
export MLFLOW_EXPERIMENT_NAME=nobodywho-evals
python main.py -m ./model.gguf
```

### Weights & Biases
```bash
export WANDB_API_KEY=your_key
python main.py -m ./model.gguf
```

### HuggingFace Hub
```bash
export HF_TOKEN=your_token
python main.py -m ./model.gguf
```

## Output

The script prints:
1. Per-task metrics (accuracy, pass@1, F1, etc.)
2. Failure summary if any generations failed
3. Sample outputs (with `--print-samples`)

System info (CPU, GPU, memory) is logged to MLflow/W&B for reproducibility.
