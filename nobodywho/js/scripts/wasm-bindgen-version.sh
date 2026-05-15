#!/usr/bin/env bash
# Print the wasm-bindgen crate version pinned by nobodywho/Cargo.lock.
#
# wasm-bindgen-cli and the wasm-bindgen runtime glue baked into our .wasm must
# match exactly — a mismatch produces a binary the host can't instantiate.
# This script is the single source of truth: CI and build-pkg.sh both source
# the version from here so they can never drift from the lockfile.
#
# Usage:
#   WASM_BINDGEN_VERSION=$(bash nobodywho/js/scripts/wasm-bindgen-version.sh)
#   cargo install wasm-bindgen-cli --version "$WASM_BINDGEN_VERSION" --locked
set -euo pipefail

# scripts/ → js/ → nobodywho/
LOCKFILE="$(cd "$(dirname "$0")/../.." && pwd)/Cargo.lock"

if [[ ! -f "$LOCKFILE" ]]; then
  echo "error: $LOCKFILE not found" >&2
  exit 1
fi

# Grab the line directly after `name = "wasm-bindgen"` (top-level, not
# wasm-bindgen-futures / wasm-bindgen-macro / etc.).
version="$(awk '
  /^\[\[package\]\]/   { in_pkg = 1; name = ""; ver = ""; next }
  in_pkg && /^name = / { name = $0 }
  in_pkg && /^version = / && name == "name = \"wasm-bindgen\"" {
    gsub(/^version = "|"$/, "", $0); print; exit
  }
' "$LOCKFILE")"

if [[ -z "$version" ]]; then
  echo "error: could not find wasm-bindgen entry in $LOCKFILE" >&2
  exit 1
fi

echo "$version"
