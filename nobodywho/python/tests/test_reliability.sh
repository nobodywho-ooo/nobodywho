#!/bin/bash

# Usage: ./test_reliability.sh [-n iterations] path/to/script.py [args...]
set -o errexit

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python "$SCRIPT_DIR/reliability_runner.py" "$@"
