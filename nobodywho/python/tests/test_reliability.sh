#!/bin/bash

# Script to run a Python script N times and count non-zero exit codes
# Usage: ./test_reliability.sh [-n iterations] path/to/script.py [args...]

# Default number of iterations
TOTAL_RUNS=100

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -n|--iterations)
            TOTAL_RUNS="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [-n iterations] <python_script> [args...]"
            echo "  -n, --iterations  Number of times to run the script (default: 100)"
            echo "  -h, --help        Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0 script.py"
            echo "  $0 -n 50 script.py model.gguf"
            echo "  $0 --iterations 200 nobodywho/python/small_model_test.py model.gguf"
            exit 0
            ;;
        *)
            SCRIPT_PATH="$1"
            shift
            break
            ;;
    esac
done

if [ -z "$SCRIPT_PATH" ]; then
    echo "Error: Python script path is required"
    echo "Usage: $0 [-n iterations] <python_script> [args...]"
    echo "Use -h for help"
    exit 1
fi

# Validate iterations is a positive integer
if ! [[ "$TOTAL_RUNS" =~ ^[1-9][0-9]*$ ]]; then
    echo "Error: Iterations must be a positive integer, got '$TOTAL_RUNS'"
    exit 1
fi

# Check if script exists
if [ ! -f "$SCRIPT_PATH" ]; then
    echo "Error: Script '$SCRIPT_PATH' not found"
    exit 1
fi

# Initialize counters
SUCCESS_COUNT=0
FAILURE_COUNT=0
FAILURES=()

echo "Running '$SCRIPT_PATH' $TOTAL_RUNS times..."
echo "Arguments: $@"
echo ""

# Progress bar function
progress_bar() {
    local current=$1
    local total=$2
    local width=50
    local percentage=$((current * 100 / total))
    local filled=$((current * width / total))
    
    printf "\rProgress: ["
    printf "%*s" $filled | tr ' ' '='
    printf "%*s" $((width - filled)) | tr ' ' '-'
    printf "] %d%% (%d/%d)" $percentage $current $total
}

# Run the script 100 times
for i in $(seq 1 $TOTAL_RUNS); do
    progress_bar $i $TOTAL_RUNS
    
    # Run the Python script and capture exit code
    python "$SCRIPT_PATH" "$@" # > /dev/null 2>&1
    EXIT_CODE=$?
    
    if [ $EXIT_CODE -eq 0 ]; then
        SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
    else
        FAILURE_COUNT=$((FAILURE_COUNT + 1))
        FAILURES+=("Run $i: exit code $EXIT_CODE")
    fi
done

echo ""
echo ""
echo "=== RESULTS ==="
echo "Total runs: $TOTAL_RUNS"
echo "Successes: $SUCCESS_COUNT"
echo "Failures: $FAILURE_COUNT"
echo "Success rate: $(( SUCCESS_COUNT * 100 / TOTAL_RUNS ))%"
echo "Failure rate: $(( FAILURE_COUNT * 100 / TOTAL_RUNS ))%"

if [ $FAILURE_COUNT -gt 0 ]; then
    echo ""
    echo "=== FAILURE DETAILS ==="
    for failure in "${FAILURES[@]}"; do
        echo "$failure"
    done
fi

# Exit with failure count as exit code (capped at 255)
exit $((FAILURE_COUNT > 255 ? 255 : FAILURE_COUNT))
