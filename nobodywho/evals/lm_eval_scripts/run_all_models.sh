#!/usr/bin/env bash
# Run evaluations on all GGUF models in a directory
#
# Usage: ./run_all_models.sh -d <model_dir> [-l <limit>] [--mlflow]
#   model_dir: Directory containing .gguf model files
#   limit: Number of samples per task (default: no limit)
#   --mlflow: Enable MLflow logging

set -e

# Default values
MODEL_DIR=""
LIMIT=""
MLFLOW_FLAG=""

usage() {
    echo "Usage: $0 -d <model_dir> [-l <limit>] [--mlflow]"
    echo ""
    echo "Options:"
    echo "  -d, --dir      Directory containing .gguf model files (required)"
    echo "  -l, --limit    Number of samples per task (default: no limit)"
    echo "  --mlflow       Enable MLflow logging"
    echo "  -h, --help     Show this help message"
    echo ""
    echo "Example:"
    echo "  $0 -d /path/to/models -l 100 --mlflow"
    exit 1
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -d|--dir)
            MODEL_DIR="$2"
            shift 2
            ;;
        -l|--limit)
            LIMIT="$2"
            shift 2
            ;;
        --mlflow)
            MLFLOW_FLAG="--mlflow"
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

# Validate model directory
if [[ -z "$MODEL_DIR" ]]; then
    echo "Error: Model directory required. Use -d or --dir."
    usage
fi

if [[ ! -d "$MODEL_DIR" ]]; then
    echo "Error: Directory not found: $MODEL_DIR"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Find all GGUF models
MODELS=($(find "$MODEL_DIR" -maxdepth 1 -name "*.gguf" -type f | sort))
TOTAL_MODELS=${#MODELS[@]}

if [[ $TOTAL_MODELS -eq 0 ]]; then
    echo "Error: No .gguf files found in $MODEL_DIR"
    exit 1
fi

# Build args for run_evals.sh
EVAL_ARGS=""
if [[ -n "$LIMIT" ]]; then
    EVAL_ARGS="$EVAL_ARGS -l $LIMIT"
fi
if [[ -n "$MLFLOW_FLAG" ]]; then
    EVAL_ARGS="$EVAL_ARGS --mlflow"
fi

# Helper to format seconds as HH:MM:SS
format_time() {
    local seconds=$1
    printf "%02d:%02d:%02d" $((seconds/3600)) $((seconds%3600/60)) $((seconds%60))
}

echo "=============================================="
echo "Multi-Model Evaluation Suite"
echo "=============================================="
echo "Directory: $MODEL_DIR"
echo "Models found: $TOTAL_MODELS"
echo "Limit: ${LIMIT:-none}"
echo "MLflow: ${MLFLOW_FLAG:-disabled}"
echo "=============================================="
echo ""
echo "Models to evaluate:"
for model in "${MODELS[@]}"; do
    echo "  - $(basename "$model")"
done
echo ""

# Track timing
SUITE_START=$(date +%s)
CURRENT_MODEL=0

# Run evals for each model
for model in "${MODELS[@]}"; do
    CURRENT_MODEL=$((CURRENT_MODEL + 1))
    MODEL_NAME=$(basename "$model" .gguf)
    MODEL_START=$(date +%s)

    echo "############################################"
    echo "# Model [$CURRENT_MODEL/$TOTAL_MODELS]: $MODEL_NAME"
    echo "############################################"
    echo ""

    # Run evals with model-specific output file
    "$SCRIPT_DIR/run_evals.sh" -m "$model" -o "results_${MODEL_NAME}.txt" $EVAL_ARGS

    # Calculate model time
    MODEL_END=$(date +%s)
    MODEL_DURATION=$((MODEL_END - MODEL_START))
    ELAPSED=$((MODEL_END - SUITE_START))

    echo ""
    echo "Model completed in $(format_time $MODEL_DURATION) | Total elapsed: $(format_time $ELAPSED)"
    echo ""
done

# Final summary
SUITE_END=$(date +%s)
TOTAL_DURATION=$((SUITE_END - SUITE_START))

echo "############################################"
echo "# All $TOTAL_MODELS models complete!"
echo "# Total time: $(format_time $TOTAL_DURATION)"
echo "############################################"
echo ""
echo "Results files:"
for model in "${MODELS[@]}"; do
    MODEL_NAME=$(basename "$model" .gguf)
    echo "  - results_${MODEL_NAME}.txt"
done
if [[ -n "$MLFLOW_FLAG" ]]; then
    echo ""
    echo "View MLflow results:"
    echo "  ./mlflow_ui.sh"
fi
