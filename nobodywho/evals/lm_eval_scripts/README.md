# NobodyWho LM Evaluation Harness

This script runs the [lm-eval-harness](https://github.com/EleutherAI/lm-evaluation-harness) benchmarks using the `nobodywho` inference backend.

## Quick Start

```bash
# Run full eval suite
python main.py /path/to/model.gguf

# Run specific tasks with sample limit
python main.py /path/to/model.gguf -t gsm8k,mbpp -l 100

# Run multiple models
python main.py /path/to/model1.gguf /path/to/model2.gguf -l 100
```

## CLI Arguments

| Argument | Short | Description |
|----------|-------|-------------|
| `MODELS` | | Path(s) to GGUF model files (positional, required) |
| `--tasks` | `-t` | Comma-separated list of tasks (default: all) |
| `--limit` | `-l` | Number of samples per task (default: no limit) |
| `--output` | `-o` | CSV output template (default: `results_{model}.csv`) |
| `--system-prompt` | | Override system prompt for ALL tasks |
| `--no-system-prompts` | | Disable all built-in per-task prompts |
| `--print-samples` | | Print prompts and responses after evaluation |
| `--seed` | | Random seed (default: 42) |

## Examples

```bash
# Run GSM8K with 50 samples and print outputs
python main.py ./model.gguf -t gsm8k -l 50 --print-samples

# Run code benchmarks only
python main.py ./model.gguf -t humaneval,mbpp

# Run full suite with 500 samples each
python main.py ./model.gguf -l 500

# Custom output file
python main.py ./model.gguf -l 100 -o my_results.csv

# Override system prompt for all tasks
python main.py ./model.gguf -t drop --system-prompt "Answer briefly."
```

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

Note: DROP automatically uses random sampling (seed=42) due to passage grouping bias.

## CSV Output

Results are saved to a CSV file per model (default: `results_{model}.csv`). Each run appends a new row.

**Columns:**

| Column | Description |
|--------|-------------|
| `timestamp` | When the run completed |
| `model_path` | Full path to the model file |
| `model_name` | Model filename (stem) |
| `model_size_gb` | Model file size in GB |
| `limit` | Sample limit used |
| `seed` | Random seed |
| `duration_seconds` | Total run time |
| `total_samples` | Total samples evaluated |
| `failed_samples` | Number of failed generations |
| `failure_rate` | Failed / total samples |
| `{task}_{metric}` | Metric value (empty if task not run) |
| `cpu_model` | CPU model name |
| `cpu_count` | Physical CPU cores |
| `memory_total_gb` | Total system memory |
| `gpu_device` | GPU device info |
| `os` | Operating system |
| `nobodywho_version` | Package version |
| `nobodywho_commit` | Git commit (if editable install) |

**Metric columns:**

- `ifeval_prompt_level_strict_acc`, `ifeval_inst_level_strict_acc`, etc.
- `gsm8k_exact_match__strict-match`, `gsm8k_exact_match__flexible-extract`
- `truthfulqa_gen_bleu_max`, `truthfulqa_gen_bleu_acc`, `truthfulqa_gen_bleu_diff`
- `humaneval_pass_at_1__create_test`
- `mbpp_pass_at_1`
- `drop_f1`, `drop_em`

Tasks not run in a given row have empty values for their metrics.

## Model Adapter

The `NobodyWhoLM` class adapts `nobodywho.Chat` to the lm-eval interface:

- **Thinking support**: Limits (max tokens, stop sequences) only enforced after `</think>` block
- **Stop sequences**: Uses `chat.stop_generation()` for early termination
- **Failure tracking**: Logs failed samples with error details

## Output

The script prints:
1. Per-task metrics (accuracy, pass@1, F1, etc.)
2. Failure summary if any generations failed
3. Sample outputs (with `--print-samples`)
