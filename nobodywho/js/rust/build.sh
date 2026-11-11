#!/usr/bin/env bash

# Build the Emscripten module for the Node.js/web build.
#
# Usage: ./rust/build.sh [debug|release]
#
# Requires the `wasm32-unknown-emscripten` target and the Emscripten SDK.

set -euo pipefail

cd "$(dirname "$0")"

profile="debug"
cargo_args=""
case "${1:-debug}" in
  debug) profile="debug" ;;
  release) profile="release"; cargo_args="--release" ;;
  *) echo "usage: $0 [debug|release]" >&2; exit 1 ;;
esac

# FIXME(madsmtm): Remove the need for this by fixing it in cc-rs.
export CFLAGS_wasm32_unknown_emscripten=-fPIC
export CXXFLAGS_wasm32_unknown_emscripten=-fPIC

# Build the Emscripten .js and .wasm files.
cargo build --target=wasm32-unknown-emscripten --package=nobodywho-js $cargo_args

# Copy the outputs.
out="../../target/wasm32-unknown-emscripten/$profile"
cp "$out/nobodywho_minimal.js" ../src/uniffi.js
cp "$out/nobodywho_minimal.wasm" ../src/nobodywho_minimal.wasm
echo "Copied $out/nobodywho_minimal.{js,wasm}"
