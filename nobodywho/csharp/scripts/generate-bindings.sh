#!/usr/bin/env bash
# Regenerate the C# bindings (src/NobodyWho/generated/nobodywho.cs) from a built
# nobodywho-uniffi library.
#
# Usage: csharp/scripts/generate-bindings.sh <path to libnobodywho_uniffi.{so,dylib,dll}>
#
# Run from anywhere. Installs the pinned uniffi-bindgen-cs into
# nobodywho/target/tools on first use, unless a matching one is on PATH.
set -euo pipefail

# Last uniffi-bindgen-cs commit targeting UniFFI 0.30, matching nobodywho-uniffi.
# Move to a tagged release once the workspace moves to UniFFI 0.31.
BINDGEN_REV=1f677ed60839cf9cf7fbcdd305b2eeaae29d84c3
BINDGEN_VERSION="0.10.0+v0.30.0"

lib=$(realpath "$1")
csharp_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
workspace_dir=$(dirname "$csharp_dir")
tools_dir="$workspace_dir/target/tools"
out_dir="$csharp_dir/src/NobodyWho/generated"
out="$out_dir/nobodywho.cs"

bindgen=$(command -v uniffi-bindgen-cs || true)
if ! "${bindgen:-false}" --version 2>/dev/null | grep -qF "$BINDGEN_VERSION"; then
    bindgen="$tools_dir/bin/uniffi-bindgen-cs"
fi
if ! "$bindgen" --version 2>/dev/null | grep -qF "$BINDGEN_VERSION"; then
    echo "Installing uniffi-bindgen-cs $BINDGEN_VERSION into $tools_dir"
    cargo install uniffi-bindgen-cs --locked --root "$tools_dir" \
        --git https://github.com/NordSecurity/uniffi-bindgen-cs --rev "$BINDGEN_REV"
fi

"$bindgen" --library "$lib" --config "$csharp_dir/uniffi.toml" --out-dir "$out_dir" --no-format

# Fixes for bugs in this generator version (all fixed upstream in v0.11):
# - Jagged arrays are allocated as `new T[][(length)]`, which does not compile.
sed -E -i.bak 's/new ([A-Za-z0-9_.]+)\[\]\[\(length\)\]/new \1[length][]/g' "$out"
rm -f "$out.bak"

# The generated layer is not ours to lint: its doc comments are copied from
# rustdoc (not valid XML docs) and it trips nullability warnings.
{
    head -n 4 "$out"
    echo
    echo "#pragma warning disable"
    tail -n +5 "$out"
} > "$out.tmp"
mv "$out.tmp" "$out"
