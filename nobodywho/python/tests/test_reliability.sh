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

# Convert script path to absolute path (needed for cd to core dir)
SCRIPT_PATH=$(realpath "$SCRIPT_PATH")

# Convert remaining arguments to absolute paths (in case they're file paths)
SCRIPT_ARGS=()
for arg in "$@"; do
    if [ -f "$arg" ] || [ -d "$arg" ]; then
        SCRIPT_ARGS+=("$(realpath "$arg")")
    else
        SCRIPT_ARGS+=("$arg")
    fi
done

# Setup core dump capture
CORE_DIR="/tmp/cores_${SCRIPT_PATH##*/}_$$"
mkdir -p "$CORE_DIR"
ulimit -c unlimited

# Set core pattern (try both methods for compatibility)
if [ -w /proc/sys/kernel/core_pattern ]; then
    echo "$CORE_DIR/core.%e.%p.%t" | sudo tee /proc/sys/kernel/core_pattern > /dev/null 2>&1
    CORE_PATTERN_SET="system-wide"
elif sudo -n true 2>/dev/null && [ -w /proc/sys/kernel/core_pattern ] 2>/dev/null; then
    echo "$CORE_DIR/core.%e.%p.%t" | sudo tee /proc/sys/kernel/core_pattern > /dev/null 2>&1
    CORE_PATTERN_SET="system-wide"
else
    # Fallback: cores will appear in current directory or system default location
    CORE_PATTERN_SET="default ($(cat /proc/sys/kernel/core_pattern 2>/dev/null || echo 'unknown'))"
fi

echo "Core dumps enabled: $CORE_PATTERN_SET"
echo "Core dump directory: $CORE_DIR"
echo ""

# Create timestamp file for tracking when test started
touch /tmp/test_reliability_start_$$

# Cleanup handler to preserve core dump info
cleanup() {
    if [ -d "$CORE_DIR" ] && [ "$(ls -A $CORE_DIR 2>/dev/null)" ]; then
        echo ""
        echo "Core dumps preserved in: $CORE_DIR"
    fi
}
trap cleanup EXIT

# Initialize counters
SUCCESS_COUNT=0
FAILURE_COUNT=0
FAILURES=()

echo "Running '$SCRIPT_PATH' $TOTAL_RUNS times..."
echo "Arguments: ${SCRIPT_ARGS[@]}"
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

# Run the script N times
for i in $(seq 1 $TOTAL_RUNS); do
    progress_bar $i $TOTAL_RUNS

    # Count cores before this run
    CORES_BEFORE=$(find "$CORE_DIR" -name "core.*" 2>/dev/null | wc -l)

    # Run the Python script in a subshell to ensure core dumps are captured
    (
        ulimit -c unlimited
        cd "$CORE_DIR"  # Run from core directory so dumps appear here
        exec python -X faulthandler "$SCRIPT_PATH" "${SCRIPT_ARGS[@]}" 2>&1
    ) | tee -a /tmp/crash_log_$i.txt

    EXIT_CODE=${PIPESTATUS[0]}

    # Check if a new core dump was created
    CORES_AFTER=$(find "$CORE_DIR" -name "core.*" 2>/dev/null | wc -l)
    LATEST_CORE=""

    if [ $CORES_AFTER -gt $CORES_BEFORE ]; then
        LATEST_CORE=$(find "$CORE_DIR" -name "core.*" -type f -printf '%T@ %p\n' 2>/dev/null | sort -nr | head -1 | cut -d' ' -f2-)
        if [ -z "$LATEST_CORE" ]; then
            # Fallback for systems without -printf
            LATEST_CORE=$(ls -t "$CORE_DIR"/core.* 2>/dev/null | head -1)
        fi
    fi

    if [ $EXIT_CODE -eq 0 ]; then
        SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
    else
        FAILURE_COUNT=$((FAILURE_COUNT + 1))
        if [ -n "$LATEST_CORE" ]; then
            echo "  [CORE DUMP: $(basename $LATEST_CORE)]" >&2
            FAILURES+=("Run $i: exit code $EXIT_CODE [CORE: $LATEST_CORE]")
        else
            FAILURES+=("Run $i: exit code $EXIT_CODE")
        fi
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

# Check for any core dumps
CORE_COUNT=$(find "$CORE_DIR" -name "core.*" -type f 2>/dev/null | wc -l)

if [ $CORE_COUNT -gt 0 ]; then
    echo ""
    echo "=== CORE DUMPS CAPTURED ==="
    echo "Core dumps saved in: $CORE_DIR"
    echo "Count: $CORE_COUNT"
    echo ""
    echo "To analyze a core dump, run:"
    echo "  gdb python $CORE_DIR/core.* -batch -ex 'bt full' -ex 'thread apply all bt'"
    echo ""
    echo "To see frames with the std::out_of_range exception:"
    echo "  gdb python $CORE_DIR/core.* -batch -ex 'bt full' 2>&1 | grep -C 20 'unordered_map'"
    echo ""
    echo "Core dump files:"
    ls -lh "$CORE_DIR"/core.* 2>/dev/null
else
    echo ""
    echo "No core dumps were captured."
    if [ $FAILURE_COUNT -gt 0 ]; then
        echo "Note: Failures occurred but no cores were generated."
        echo "This might happen if:"
        echo "  - Core pattern couldn't be set (try running with sudo)"
        echo "  - Python caught the signal before core dump"
        echo "  - System core dumps are disabled"
        echo ""
        echo "Try running a single instance under GDB:"
        echo "  gdb --args python -X faulthandler $SCRIPT_PATH ${SCRIPT_ARGS[@]}"
        echo "  (gdb) catch throw std::out_of_range"
        echo "  (gdb) run"
    fi
fi

# Exit with failure count as exit code (capped at 255)
exit $((FAILURE_COUNT > 255 ? 255 : FAILURE_COUNT))
