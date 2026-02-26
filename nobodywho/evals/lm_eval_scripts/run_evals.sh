#!/usr/bin/env bash
set -e

# Default values
LIMIT=""
MODEL="${TEST_MODEL:-}"
RESULTS_FILE="results.txt"

usage() {
    echo "Usage: $0 -m <model_path> [-l <limit>] [-o <output>]"
    echo ""
    echo "Options:"
    echo "  -m, --model    Path to GGUF model file (or set TEST_MODEL env var)"
    echo "  -l, --limit    Number of samples per task (default: no limit)"
    echo "  -o, --output   Results output file (default: results.txt)"
    echo "  -h, --help     Show this help message"
    echo ""
    echo "Note: DROP automatically uses random sampling (seed=42) due to passage grouping."
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

# Task -> prompt file mapping
declare -A TASK_PROMPTS=(
    ["ifeval"]=""
    ["gsm8k"]="gsm8k.txt"
    ["truthfulqa_gen"]="truthfulqa.txt"
    ["humaneval"]="humaneval.txt"
    ["mbpp"]="mbpp.txt"
    ["drop"]="drop.txt"
)

# Initialize results file
cat > "$RESULTS_FILE" << EOF
==============================================
NobodyWho Eval Suite Results
==============================================
Model: $MODEL
Limit: ${LIMIT:-none}
Date: $(date)
==============================================

EOF

echo "=============================================="
echo "NobodyWho Eval Suite"
echo "=============================================="
echo "Model: $MODEL"
echo "Limit: ${LIMIT:-none}"
echo "Results: $RESULTS_FILE"
echo "=============================================="
echo ""

# Run each task
for task in ifeval gsm8k truthfulqa_gen humaneval mbpp drop; do
    echo "----------------------------------------------"
    echo "Running: $task"
    echo "----------------------------------------------"

    PROMPT_FILE="${TASK_PROMPTS[$task]}"
    PROMPT_ARG=""
    TASK_ARGS=""

    if [[ -n "$PROMPT_FILE" && -f "$PROMPTS_DIR/$PROMPT_FILE" ]]; then
        PROMPT_ARG="--system-prompt-file $PROMPTS_DIR/$PROMPT_FILE"
        echo "Using prompt: $PROMPT_FILE"
    else
        PROMPT_ARG="--system-prompt ''"
        echo "No system prompt"
    fi

    # DROP uses random sampling due to passage grouping bias
    if [[ "$task" == "drop" ]]; then
        TASK_ARGS="--shuffle --seed 42"
        echo "Using random sampling (seed=42)"
    fi

    # Run eval and capture output
    OUTPUT=$(uv run python "$SCRIPT_DIR/main.py" -t "$task" $COMMON_ARGS $PROMPT_ARG $TASK_ARGS 2>&1)

    echo "$OUTPUT"

    # Extract and save results to file
    echo "--- $task ---" >> "$RESULTS_FILE"
    echo "$OUTPUT" | grep -E "^  (alias|pass|exact|bleu|f1|prompt|inst)" >> "$RESULTS_FILE" || true
    echo "" >> "$RESULTS_FILE"

    echo ""
done

echo "=============================================="
echo "All evals complete!"
echo "Results saved to: $RESULTS_FILE"
echo "=============================================="
