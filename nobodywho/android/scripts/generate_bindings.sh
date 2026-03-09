#!/bin/bash
set -e

# Generate Kotlin bindings from the UDL using uniffi-bindgen.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ANDROID_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$(cd "$ANDROID_DIR/.." && pwd)"
UNIFFI_DIR="$ROOT_DIR/uniffi"
OUTPUT_DIR="$ANDROID_DIR/library/src/main/kotlin"

echo "Generating Kotlin bindings..."
echo "  UDL:    $UNIFFI_DIR/src/nobodywho.udl"
echo "  Output: $OUTPUT_DIR"

# Build the uniffi library first so the bindgen can introspect it.
echo "Building Rust library..."
cargo build -p nobodywho-uniffi --manifest-path "$ROOT_DIR/Cargo.toml"

# Run the bindgen.
cargo run \
    --manifest-path "$ANDROID_DIR/bindgen/Cargo.toml" \
    -- \
    generate \
    --language kotlin \
    --out-dir "$OUTPUT_DIR" \
    "$UNIFFI_DIR/src/nobodywho.udl"

echo ""
echo "Kotlin bindings written to: $OUTPUT_DIR/uniffi/nobodywho/"
