#!/bin/bash
# Combine the embedded ggml/llama libs from two single-arch build dirs into <out-dir> for
# make-apple-framework.sh. Libs present in both arches (the ggml/llama dylibs and the Metal
# backend module) are lipo'd into universal binaries; the per-microarch CPU backend modules
# (libggml-cpu-apple_m*.so on arm64, libggml-cpu-<x86>.so on x86_64) exist in only one arch
# and are copied as-is — they can't be lipo'd, and ggml picks the right one per host at
# runtime. The caller lipos the main cdylib itself (its filename differs CI vs local).
#
# Usage: lipo-apple-libs.sh <arch-dir-a> <arch-dir-b> <out-dir>
set -euo pipefail
A=$1; B=$2; OUT=$3
mkdir -p "$OUT"
shopt -s nullglob
# Union of ggml/llama basenames across both arches (dylibs + dlopen'd .so modules).
names=$(for f in "$A"/libggml* "$A"/libllama* "$B"/libggml* "$B"/libllama*; do basename "$f"; done | sort -u)
n=0
for name in $names; do
    if [ -e "$A/$name" ] && [ -e "$B/$name" ]; then
        lipo -create "$A/$name" "$B/$name" -output "$OUT/$name"
    elif [ -e "$A/$name" ]; then
        cp -L "$A/$name" "$OUT/$name"
    else
        cp -L "$B/$name" "$OUT/$name"
    fi
    n=$((n + 1))
done
# An empty union means no ggml at all downstream — a load failure on the consumer's
# machine, not here — so fail loudly instead.
if [ "$n" -eq 0 ]; then
    echo "lipo-apple-libs: no libggml*/libllama* files found in $A or $B" >&2
    exit 1
fi
