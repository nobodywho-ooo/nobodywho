#!/bin/bash
# Stage the dynamically-linked ggml/llama ELF shared libs into a directory for
# shipping: copy them next to the consuming binding, and optionally give each an
# $ORIGIN runpath.
#
# The libs are born UNVERSIONED (libggml.so, not libggml.so.0) via the
# reset-soversion CMake override — see nobodywho/scripts/llama-build-overrides.cmake
# — so no SONAME / DT_NEEDED rewriting is needed here.
#
# Usage: stage-elf-libs.sh [--from <src-dir>] [--origin] <dir>
#   --from <src-dir>  Copy the ggml/llama .so from <src-dir> into <dir> first.
#   --origin          Set an $ORIGIN runpath on each lib (needed by Linux co-located
#                     consumers: DT_RUNPATH does NOT chain to transitive deps, so each
#                     lib needs its own to find its siblings, libggml -> libggml-base).
#                     Requires patchelf. Not needed on Android (loader uses jniLibs).
set -euo pipefail
from=""; origin=0
while [ $# -gt 0 ]; do
    case "$1" in
        --from) from="$2"; shift 2 ;;
        --origin) origin=1; shift ;;
        *) break ;;
    esac
done
dir="$1"
shopt -s nullglob

if [ -n "$from" ]; then
    find "$from" \( -name 'libggml*.so' -o -name 'libllama*.so' \) ! -type l \
        -exec cp -n {} "$dir/" \;
fi

if [ "$origin" = 1 ]; then
    command -v patchelf >/dev/null || { echo "patchelf not found" >&2; exit 1; }
    for lib in "$dir"/libggml*.so "$dir"/libllama*.so; do
        [ -e "$lib" ] && patchelf --set-rpath '$ORIGIN' "$lib"
    done
fi

echo "staged ggml/llama libs in $dir"
