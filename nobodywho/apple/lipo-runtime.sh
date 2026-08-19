#!/bin/bash
# Usage: lipo-runtime.sh <arch-dir-a> <arch-dir-b> <out-dir>
set -euo pipefail
shopt -s nullglob

A="$1/nobodywho-runtime"
B="$2/nobodywho-runtime"
OUT="$3/nobodywho-runtime"
rm -rf "$OUT"
mkdir -p "$OUT"

libraries=("$A"/*.dylib)
[ ${#libraries[@]} -gt 0 ] || { echo "no runtime libraries in $A" >&2; exit 1; }
for file in "${libraries[@]}"; do
    name=${file##*/}
    [ -f "$B/$name" ] || { echo "$name is missing from $B" >&2; exit 1; }
    lipo -create "$file" "$B/$name" -output "$OUT/$name"
done
for file in "$B"/*.dylib; do
    [ -f "$OUT/${file##*/}" ] || { echo "${file##*/} is missing from $A" >&2; exit 1; }
done

backends=("$A"/*.so "$B"/*.so)
[ ${#backends[@]} -gt 0 ] || { echo "no backend modules in $A or $B" >&2; exit 1; }
for file in "${backends[@]}"; do
    name=${file##*/}
    [ -e "$OUT/$name" ] && continue
    if [ -f "$A/$name" ] && [ -f "$B/$name" ]; then
        lipo -create "$A/$name" "$B/$name" -output "$OUT/$name"
    else
        cp -L "$file" "$OUT/$name"
    fi
done
