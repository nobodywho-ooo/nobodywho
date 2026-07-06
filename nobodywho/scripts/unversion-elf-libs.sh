#!/bin/bash
# Normalize the dynamically-linked ggml/llama ELF shared libraries in a directory
# to UNVERSIONED SONAMEs (libggml.so instead of libggml.so.0 / .so.0.13.1) and
# repoint every consumer's DT_NEEDED to match.
#
# Why: on Linux/Android llama.cpp emits versioned SONAMEs, but (a) Android APKs
# cannot package versioned .so.N filenames and the loader needs unversioned
# SONAMEs, and (b) our packaging (Godot .gdextension [dependencies], Flutter
# resolve_binary, jniLibs globs) all reference unversioned names. Normalizing
# here — rather than in the llama-cpp-rs fork — keeps the change consumer-side.
#
# Requires: patchelf. Usage: unversion-elf-libs.sh <dir> [extra-binary ...]
# extra-binary: additional ELF files (e.g. an executable) whose DT_NEEDED should
# also be repointed to the unversioned ggml/llama names.
set -euo pipefail
dir="$1"; shift || true
extra_bins=("$@")
command -v patchelf >/dev/null || { echo "patchelf not found" >&2; exit 1; }
shopt -s nullglob

# 1) Reduce each versioned ggml/llama file to a single unversioned real file.
for f in "$dir"/libggml*.so.* "$dir"/libllama*.so.*; do
    bn=$(basename "$f")
    base="${bn%%.so.*}.so"            # libggml-cpu.so.0.13.1 -> libggml-cpu.so
    tmp="$dir/.$base.tmp"
    cp -L "$f" "$tmp"                  # dereference to the real object
    mv -f "$tmp" "$dir/$base"
done
rm -f "$dir"/libggml*.so.* "$dir"/libllama*.so.*   # drop the versioned variants

# 2) Set an unversioned SONAME on each ggml/llama lib.
for lib in "$dir"/libggml*.so "$dir"/libllama*.so; do
    [ -e "$lib" ] || continue
    patchelf --set-soname "$(basename "$lib")" "$lib"
done

# 3) Repoint every ELF object's DT_NEEDED for ggml/llama from versioned to
#    unversioned (covers the binding cdylib, the exe, and inter-ggml deps).
for so in "$dir"/*.so ${extra_bins[@]+"${extra_bins[@]}"}; do
    [ -e "$so" ] || continue
    for n in $(patchelf --print-needed "$so" 2>/dev/null || true); do
        case "$n" in
            libggml*.so.*|libllama*.so.*)
                patchelf --replace-needed "$n" "${n%%.so.*}.so" "$so" ;;
        esac
    done
done

echo "unversioned ggml/llama libs in $dir"
