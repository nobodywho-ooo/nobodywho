#!/bin/bash
# Stage the dynamically-linked ggml/llama ELF shared libraries in a directory for
# shipping: normalize them to UNVERSIONED SONAMEs (libggml.so, not libggml.so.0 /
# .so.0.13.1) and repoint every consumer's DT_NEEDED to match. Optionally harvest
# them from a build dir first, and/or give each an $ORIGIN runpath.
#
# Unversioning is done here (not in the llama-cpp-rs fork) because Android APKs
# cannot package versioned .so.N and our packaging (Godot .gdextension, Flutter
# resolve_binary, jniLibs globs) all reference unversioned names.
#
# Requires: patchelf.
# Usage: stage-elf-libs.sh [--from <src-dir>] [--origin] <dir> [extra-binary ...]
#   --from <src-dir>  Harvest the real ggml/llama objects from <src-dir> into <dir>
#                     first. The fork hard-links only the unversioned dev symlinks
#                     into the cargo profile dir; the real versioned SONAME files
#                     that DT_NEEDED points at live deeper in the CMake build output.
#   --origin          Set an $ORIGIN runpath on each ggml/llama lib after unversioning.
#                     DT_RUNPATH does NOT chain to transitive deps, so each lib needs
#                     its own to find its siblings (libggml -> libggml-base, ...).
#   extra-binary      Extra ELF files whose ggml/llama DT_NEEDED should also be repointed.
set -euo pipefail

from=""; origin=0
while [ $# -gt 0 ]; do
    case "$1" in
        --from) from="$2"; shift 2 ;;
        --origin) origin=1; shift ;;
        *) break ;;
    esac
done
dir="$1"; shift || true
extra_bins=("$@")
command -v patchelf >/dev/null || { echo "patchelf not found" >&2; exit 1; }
shopt -s nullglob

# 0) Optionally harvest the real (non-symlink) versioned ggml/llama objects into $dir.
if [ -n "$from" ]; then
    find "$from" \( -name 'libggml*.so*' -o -name 'libllama*.so*' \) ! -type l \
        -exec cp -n {} "$dir/" \;
fi

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

# 3) Repoint every ELF object's ggml/llama DT_NEEDED from versioned to unversioned
#    (covers the binding cdylib, any extra binaries, and inter-ggml deps).
for so in "$dir"/*.so ${extra_bins[@]+"${extra_bins[@]}"}; do
    [ -e "$so" ] || continue
    for n in $(patchelf --print-needed "$so" 2>/dev/null || true); do
        case "$n" in
            libggml*.so.*|libllama*.so.*)
                patchelf --replace-needed "$n" "${n%%.so.*}.so" "$so" ;;
        esac
    done
done

# 4) Optionally give each ggml/llama lib an $ORIGIN runpath (see --origin above).
if [ "$origin" = 1 ]; then
    for lib in "$dir"/libggml*.so "$dir"/libllama*.so; do
        [ -e "$lib" ] && patchelf --set-rpath '$ORIGIN' "$lib"
    done
fi

echo "staged ggml/llama libs in $dir"
