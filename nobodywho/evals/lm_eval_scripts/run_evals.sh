#!/usr/bin/env bash
set -e

# Default values
LIMIT=""
MODEL="${TEST_MODEL:-}"
RESULTS_FILE="results.txt"
MLFLOW_ENABLED=""

usage() {
    echo "Usage: $0 -m <model_path> [-l <limit>] [-o <output>] [--mlflow]"
    echo ""
    echo "Options:"
    echo "  -m, --model    Path to GGUF model file (or set TEST_MODEL env var)"
    echo "  -l, --limit    Number of samples per task (default: no limit)"
    echo "  -o, --output   Results output file (default: results.txt)"
    echo "  --mlflow       Enable MLflow logging (stores in ./mlruns)"
    echo "  -h, --help     Show this help message"
    echo ""
    echo "Note: DROP automatically uses random sampling (seed=42) due to passage grouping."
    echo ""
    echo "MLflow:"
    echo "  When --mlflow is enabled, each task is logged as a separate run."
    echo "  View results with: ./mlflow_ui.sh"
    exit 1
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -m|--model)
            MODEL="$2"
            shift 2
            ;;
        -l|--limit)
            LIMIT="$2"
            shift 2
            ;;
        -o|--output)
            RESULTS_FILE="$2"
            shift 2
            ;;
        --mlflow)
            MLFLOW_ENABLED="1"
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Unknown option: $1"
            usage
            ;;
    esac
done

# Validate model path
if [[ -z "$MODEL" ]]; then
    echo "Error: Model path required. Use -m or set TEST_MODEL env var."
    usage
fi

if [[ ! -f "$MODEL" ]]; then
    echo "Error: Model file not found: $MODEL"
    exit 1
fi

# Build common args (without shuffle - that's task-specific)
COMMON_ARGS="-m $MODEL"
if [[ -n "$LIMIT" ]]; then
    COMMON_ARGS="$COMMON_ARGS -l $LIMIT"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROMPTS_DIR="$SCRIPT_DIR/prompts"

# Extract model name for MLflow experiment
MODEL_NAME=$(basename "$MODEL" .gguf)

# Setup MLflow environment if enabled
if [[ -n "$MLFLOW_ENABLED" ]]; then
    export MLFLOW_TRACKING_URI="sqlite:///$SCRIPT_DIR/mlflow.db"
    export MLFLOW_EXPERIMENT_NAME="nobodywho-evals"
fi

# Task list and count
TASKS=(ifeval gsm8k truthfulqa_gen humaneval mbpp drop)
TOTAL_TASKS=${#TASKS[@]}

# Task -> prompt file mapping
declare -A TASK_PROMPTS=(
    ["ifeval"]=""
    ["gsm8k"]="gsm8k.txt"
    ["truthfulqa_gen"]="truthfulqa.txt"
    ["humaneval"]="humaneval.txt"
    ["mbpp"]="mbpp.txt"
    ["drop"]="drop.txt"
)

# Helper to format seconds as HH:MM:SS
format_time() {
    local seconds=$1
    printf "%02d:%02d:%02d" $((seconds/3600)) $((seconds%3600/60)) $((seconds%60))
}

# Initialize results file
cat > "$RESULTS_FILE" << EOF
==============================================
NobodyWho Eval Suite Results
==============================================
Model: $MODEL
Limit: ${LIMIT:-none}
MLflow: ${MLFLOW_ENABLED:-disabled}
Date: $(date)
==============================================

EOF

echo "=============================================="
echo "NobodyWho Eval Suite"
echo "=============================================="
echo "Model: $MODEL"
echo "Limit: ${LIMIT:-none}"
echo "Tasks: ${TOTAL_TASKS}"
echo "Results: $RESULTS_FILE"
if [[ -n "$MLFLOW_ENABLED" ]]; then
    echo "MLflow: enabled (experiment: $MLFLOW_EXPERIMENT_NAME)"
fi
echo "=============================================="
echo ""

# Track timing
SUITE_START=$(date +%s)
CURRENT_TASK=0

# Run each task
for task in "${TASKS[@]}"; do
    CURRENT_TASK=$((CURRENT_TASK + 1))
    TASK_START=$(date +%s)

    echo "=============================================="
    echo "[${CURRENT_TASK}/${TOTAL_TASKS}] ${task}"
    echo "=============================================="

    PROMPT_FILE="${TASK_PROMPTS[$task]}"
    PROMPT_ARG=""
    TASK_ARGS=""

    if [[ -n "$PROMPT_FILE" && -f "$PROMPTS_DIR/$PROMPT_FILE" ]]; then
        PROMPT_ARG="--system-prompt-file $PROMPTS_DIR/$PROMPT_FILE"
        echo "Prompt: $PROMPT_FILE"
    else
        PROMPT_ARG="--system-prompt ''"
        echo "Prompt: none"
    fi

    # DROP uses random sampling due to passage grouping bias
    if [[ "$task" == "drop" ]]; then
        TASK_ARGS="--shuffle --seed 42"
        echo "Sampling: random (seed=42)"
    else
        echo "Sampling: sequential"
    fi
    echo ""

    # Run eval (output streams in real-time for tqdm progress)
    uv run python "$SCRIPT_DIR/main.py" -t "$task" $COMMON_ARGS $PROMPT_ARG $TASK_ARGS 2>&1 | tee /tmp/eval_output_$$.txt

    # Calculate task time
    TASK_END=$(date +%s)
    TASK_DURATION=$((TASK_END - TASK_START))
    ELAPSED=$((TASK_END - SUITE_START))

    echo ""
    echo "Task completed in $(format_time $TASK_DURATION) | Total elapsed: $(format_time $ELAPSED)"

    # Extract and save results to file
    echo "--- $task ($(format_time $TASK_DURATION)) ---" >> "$RESULTS_FILE"
    grep -E "^  (alias|pass|exact|bleu|f1|prompt|inst)" /tmp/eval_output_$$.txt >> "$RESULTS_FILE" || true
    echo "" >> "$RESULTS_FILE"

    echo ""
done

# Cleanup temp file
rm -f /tmp/eval_output_$$.txt

# Final summary
SUITE_END=$(date +%s)
TOTAL_DURATION=$((SUITE_END - SUITE_START))

echo "=============================================="
echo "All ${TOTAL_TASKS} tasks complete!"
echo "Total time: $(format_time $TOTAL_DURATION)"
echo "Results saved to: $RESULTS_FILE"
if [[ -n "$MLFLOW_ENABLED" ]]; then
    echo ""
    echo "View MLflow results:"
    echo "  ./mlflow_ui.sh"
fi
echo "=============================================="
