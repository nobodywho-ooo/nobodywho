#!/bin/bash
# lipo every embedded ggml/llama dylib (dynamic-link) across two single-arch build
# dirs into <out-dir>, producing universal copies for make-apple-framework.sh. The
# caller lipos the main cdylib itself, since its filename differs between per-triple
# CI artifacts and local crate-named builds.
#
# Usage: lipo-apple-libs.sh <arch-dir-a> <arch-dir-b> <out-dir>
set -euo pipefail
A=$1; B=$2; OUT=$3
mkdir -p "$OUT"
shopt -s nullglob
for f in "$A"/libggml*.dylib "$A"/libllama*.dylib; do
    b=$(basename "$f")
    lipo -create "$f" "$B/$b" -output "$OUT/$b"
done
