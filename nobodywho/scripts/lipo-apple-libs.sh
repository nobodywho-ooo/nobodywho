#!/bin/bash
# Usage: lipo-apple-libs.sh <arch-dir-a> <arch-dir-b> <out-dir>
set -euo pipefail
A=$1; B=$2; OUT=$3
mkdir -p "$OUT"
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
RUNTIME_TOOL="$SCRIPT_DIR/runtime-bundle.py"
MANIFEST_A="$A/nobodywho-runtime/manifest.json"
MANIFEST_B="$B/nobodywho-runtime/manifest.json"
RUNTIME_OUT="$OUT/nobodywho-runtime"
rm -rf "$RUNTIME_OUT"
mkdir -p "$RUNTIME_OUT"

linked_a=$(python3 "$RUNTIME_TOOL" names "$MANIFEST_A" --kind libraries)
linked_b=$(python3 "$RUNTIME_TOOL" names "$MANIFEST_B" --kind libraries)
if [ "$linked_a" != "$linked_b" ]; then
    echo "lipo-apple-libs: linked runtime sets differ between $MANIFEST_A and $MANIFEST_B" >&2
    exit 1
fi

for name in $linked_a; do
    lipo -create \
        "$A/nobodywho-runtime/$name" \
        "$B/nobodywho-runtime/$name" \
        -output "$RUNTIME_OUT/$name"
done

# CPU backend modules can be specific to one architecture.
backend_names=$({
    python3 "$RUNTIME_TOOL" names "$MANIFEST_A" --kind backends
    python3 "$RUNTIME_TOOL" names "$MANIFEST_B" --kind backends
} | sort -u)
for name in $backend_names; do
    file_a="$A/nobodywho-runtime/$name"
    file_b="$B/nobodywho-runtime/$name"
    if [ -e "$file_a" ] && [ -e "$file_b" ]; then
        lipo -create "$file_a" "$file_b" -output "$RUNTIME_OUT/$name"
    elif [ -e "$file_a" ]; then
        cp -L "$file_a" "$RUNTIME_OUT/$name"
    else
        cp -L "$file_b" "$RUNTIME_OUT/$name"
    fi
done

python3 "$RUNTIME_TOOL" merge "$RUNTIME_OUT/manifest.json" "$MANIFEST_A" "$MANIFEST_B"
