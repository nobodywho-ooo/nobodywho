#!/usr/bin/env bash
set -euo pipefail
: "${OPENCL_INCLUDE_DIR:?Run prepare-gpu.sh first}"
source_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
test_dir="$(mktemp -d "${TMPDIR:-/tmp}/nobodywho-opencl-test.XXXXXX")"
trap 'rm -rf -- "$test_dir"' EXIT
link_flags=(-pthread)
[[ "$(uname -s)" != Linux ]] || link_flags+=(-ldl)
cc="${CC:-cc}"
"$cc" -I"$OPENCL_INCLUDE_DIR" -Wall -Wextra -Werror \
  "$source_dir/../opencl-shim.c" "$source_dir/opencl-shim-test.c" \
  "${link_flags[@]}" -o "$test_dir/test"
NOBODYWHO_OPENCL_LIBRARY="$test_dir/missing.so" "$test_dir/test" unavailable
for variant in full required optional; do
  defines=(-DMOCK_DRIVER)
  case "$variant" in
    required) defines+=(-DOMIT_REQUIRED); expected=unavailable ;;
    optional) defines+=(-DOMIT_OPTIONAL); expected=optional ;;
    full) expected=full ;;
  esac
  "$cc" -shared -fPIC -I"$OPENCL_INCLUDE_DIR" -Wall -Wextra -Werror \
    -Wno-unused-parameter "${defines[@]}" "$source_dir/opencl-shim-test.c" \
    -o "$test_dir/$variant.so"
  NOBODYWHO_OPENCL_LIBRARY="$test_dir/$variant.so" "$test_dir/test" "$expected"
done
echo 'OpenCL shim tests passed'
