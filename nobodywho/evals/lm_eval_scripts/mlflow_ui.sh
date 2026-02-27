#!/usr/bin/env bash
# Launch MLflow UI with NixOS compatibility fix
#
# Usage: ./mlflow_ui.sh [port]
#   port: Port to run on (default: 5000)

set -e

PORT="${1:-5000}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Check if we're on NixOS and need the library path fix
if [ -f /etc/NIXOS ]; then
    echo "NixOS detected, setting up LD_LIBRARY_PATH..."
    LIB_PATH=$(nix-build '<nixpkgs>' -A stdenv.cc.cc.lib --no-out-link 2>/dev/null)/lib
    export LD_LIBRARY_PATH="$LIB_PATH:$LD_LIBRARY_PATH"
fi

echo "Starting MLflow UI on http://127.0.0.1:$PORT"
echo "Press Ctrl+C to stop"
echo ""

uv run mlflow ui --backend-store-uri "sqlite:///$SCRIPT_DIR/mlflow.db" --port "$PORT"
