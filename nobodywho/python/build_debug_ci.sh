#!/bin/bash
# Build script for GitHub Actions with full debug symbols
# Usage: ./build_debug_ci.sh

set -e  # Exit on error

echo "Building nobodywho with debug symbols for CI..."
echo ""

# Set CMAKE flags for debug symbols in llama.cpp
export CMAKE_BUILD_TYPE=RelWithDebInfo
export CFLAGS="-g -O2"
export CXXFLAGS="-g -O2"

echo "CMAKE_BUILD_TYPE=$CMAKE_BUILD_TYPE"
echo "CFLAGS=$CFLAGS"
echo "CXXFLAGS=$CXXFLAGS"
echo ""

# For GitHub Actions, use pip to build and install
# This works without needing maturin installed globally
cd "$(dirname "$0")"

echo "Cleaning previous builds..."
cargo clean

echo ""
echo "Building and installing with pip..."
# Use pip install -e . for editable install, or just pip install .
pip install --verbose .

echo ""
echo "Build complete!"
echo ""
echo "To verify debug symbols:"
echo "  python -c 'import nobodywho; import nobodywho.__file__; print(nobodywho.__file__)'"
echo "  file \$(python -c 'import nobodywho; print(nobodywho.__file__)')"
