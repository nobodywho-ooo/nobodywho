#!/usr/bin/env bash

# Build the Emscripten module for the Node.js/web build.
#
# Usage: ./rust/build.sh [debug|release]
#
# Requires the `wasm32-unknown-emscripten` target and the Emscripten SDK.

set -euo pipefail

cd "$(dirname "$0")"

profile="debug"
case "${1:-debug}" in
  debug) profile="debug" ;;
  release) profile="release" ;;
  *) echo "usage: $0 [debug|release]" >&2; exit 1 ;;
esac

# FIXME(madsmtm): Remove the need for this by fixing it in cc-rs.
export CFLAGS_wasm32_unknown_emscripten=-fPIC
export CXXFLAGS_wasm32_unknown_emscripten=-fPIC

# `em++` must be the linker because we build C++ sources (llama.cpp), and not just C sources.
export CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_LINKER=em++

# See ./src/minimal.rs for why this is required.
export CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_RUSTFLAGS=-Zexport-executable-symbols
# HACK: Use RUSTC_BOOTSTRAP to avoid having a nightly compiler installed for ^.
export RUSTC_BOOTSTRAP=1

# Build the Emscripten .js and .wasm files.
cargo build --target=wasm32-unknown-emscripten --package=nobodywho-js --profile=$profile

# Copy the outputs.
out="../../target/wasm32-unknown-emscripten/$profile"
cp "$out/nobodywho_minimal.js" ../src/uniffi.js
cp "$out/nobodywho_minimal.wasm" ../src/nobodywho_minimal.wasm
echo "Copied $out/nobodywho_minimal.{js,wasm}"
