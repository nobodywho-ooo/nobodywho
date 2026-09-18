#!/usr/bin/env bash
# Inspect the linked binding, not an archive: catches accidentally resolving
# OpenCL to a shared loader, and Cargo dropping transitive static dependencies.
set -euo pipefail
library="${1:?usage: check-static.sh <binding.so>}"
readelf="${READELF:-readelf}"
dynamic="$("$readelf" --dynamic "$library")"
symbols="$("$readelf" --dyn-syms --wide "$library")"
if printf '%s\n' "$dynamic" | grep -E 'Shared library: \[lib(OpenCL|ggml[^]]*|llama[^]]*|c\+\+_shared)\.so'; then
  echo "Unexpected shared GPU/core/C++ dependency in $library" >&2
  exit 1
fi
if printf '%s\n' "$symbols" | grep -E '[[:space:]]UND[[:space:]]+cl[A-Z]' ; then
  echo "Unresolved OpenCL API calls in $library" >&2
  exit 1
fi
# The embedded loader must not interpose the vendor driver's own cl* calls.
if printf '%s\n' "$symbols" | grep -E '(GLOBAL|WEAK) +(DEFAULT|PROTECTED) +[0-9]+ +cl[A-Z]' ; then
  echo "Embedded OpenCL loader symbols must be hidden in $library" >&2
  exit 1
fi
echo "Static Android backend linkage verified: $library"
