#!/bin/bash
set -e

# Generate Swift bindings from UDL using uniffi
#
# This script compiles the Rust library with uniffi support and uses
# uniffi's built-in binding generator to create Swift code.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SWIFT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKSPACE_DIR="$(cd "$SWIFT_DIR/.." && pwd)"
CORE_DIR="$WORKSPACE_DIR/core"
OUTPUT_DIR="$SWIFT_DIR/Sources/NobodyWho/Generated"

echo "Generating Swift bindings from UDL..."
echo "  Core: $CORE_DIR"
echo "  Output: $OUTPUT_DIR"

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Build the library first to ensure it's up to date
echo "Building Rust library with UniFFI..."
cargo build -p nobodywho --features uniffi --manifest-path "$WORKSPACE_DIR/Cargo.toml"

# Use uniffi-bindgen from the project dependencies
# The uniffi crate provides a CLI for generating bindings
cd "$CORE_DIR"

# Extract the .h and .swift files from the generated scaffolding
# UniFFI generates these during the build process
echo "Extracting generated Swift bindings..."

# The generated files are in the build output directory
BUILD_DIR="$WORKSPACE_DIR/target/debug/build"

# Find the nobodywho build directory
NOBODYWHO_BUILD=$(find "$BUILD_DIR" -name "nobodywho-*" -type d | grep -v "\.d$" | head -1)

if [ -z "$NOBODYWHO_BUILD" ]; then
    echo "Error: Could not find nobodywho build directory"
    exit 1
fi

# Look for the generated uniffi files
UNIFFI_OUT="$NOBODYWHO_BUILD/out"

if [ -d "$UNIFFI_OUT" ]; then
    echo "Found UniFFI generated files in: $UNIFFI_OUT"

    # Copy generated Swift files
    if [ -f "$UNIFFI_OUT/nobodywho.swift" ]; then
        cp "$UNIFFI_OUT/nobodywho.swift" "$OUTPUT_DIR/"
        echo "✓ Copied nobodywho.swift"
    fi

    if [ -f "$UNIFFI_OUT/nobodywhoFFI.h" ]; then
        cp "$UNIFFI_OUT/nobodywhoFFI.h" "$OUTPUT_DIR/"
        echo "✓ Copied nobodywhoFFI.h"
    fi

    if [ -f "$UNIFFI_OUT/nobodywhoFFI.modulemap" ]; then
        cp "$UNIFFI_OUT/nobodywhoFFI.modulemap" "$OUTPUT_DIR/"
        echo "✓ Copied nobodywhoFFI.modulemap"
    fi
else
    echo "Warning: UniFFI output directory not found at $UNIFFI_OUT"
    echo "Attempting alternative generation method..."

    # Alternative: Use uniffi-bindgen from cargo
    # Install if not already installed
    if ! command -v uniffi-bindgen &> /dev/null; then
        echo "Installing uniffi-bindgen..."
        cargo install uniffi-bindgen --version 0.29.5 || {
            echo "Warning: Could not install uniffi-bindgen"
            echo "Please ensure UniFFI is properly configured in Cargo.toml"
        }
    fi

    # If uniffi-bindgen is available, use it directly
    if command -v uniffi-bindgen &> /dev/null; then
        uniffi-bindgen generate \
            --language swift \
            --out-dir "$OUTPUT_DIR" \
            "$CORE_DIR/src/nobodywho.udl"

        echo "✓ Generated bindings using uniffi-bindgen CLI"
    fi
fi

echo ""
echo "========================================"
echo "Swift bindings generation complete!"
echo "Output directory: $OUTPUT_DIR"
echo "========================================"
