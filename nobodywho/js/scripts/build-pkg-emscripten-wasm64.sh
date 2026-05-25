#!/usr/bin/env bash
# Build nobodywho-js for wasm64-unknown-emscripten (memory64).
#
# This is the wasm64 sibling of build-pkg-emscripten.sh. It produces a
# binary capable of using more than 4 GiB of wasm linear memory, which
# is required to run models like Gemma 3 4B (2.5 GB tensors + 1 GB mmproj
# + KV cache + compute > 4 GiB total working set).
#
# Outputs go to nobodywho/js/pkg-bundler-wasm64/ (parallel to the wasm32
# build's pkg-bundler/), so the two can coexist.
#
# Prereqs:
#   - Nightly Rust toolchain installed via rustup. wasm64-unknown-emscripten
#     is a tier-3 target consumed via a custom JSON spec at
#     targets/wasm64-unknown-emscripten.json + `-Z build-std`.
#   - **One-line rustlib patch.** library/unwind/src/libunwind.rs in the
#     nightly rust-src needs a wasm64-emscripten cfg arm for
#     `unwinder_private_data_size` (1 line). Upstream PR pending; until
#     it lands, run `scripts/patch-rustlib-wasm64.sh` (TODO) or apply
#     manually from the patch at:
#       https://github.com/nobodywho-ooo/rust/commit/1958efbecba5106dfb95d9c93412fd132eab5b76
#   - Same Emscripten + wasm-bindgen forks as the wasm32 build. Plus a
#     one-time `embuilder build SYSTEM --wasm64 -f` to populate the
#     wasm64 sysroot with libcxx-wasmexcept variants (~6 min, cached).
#   - llama-cpp-rs branch `wasm-emscripten` pinned in core/Cargo.toml
#     must have the wasm64 build.rs additions (target string recognition,
#     -sMEMORY64=1, -fwasm-exceptions, EMSCRIPTEN_SYSTEM_PROCESSOR).
#
# What differs from build-pkg-emscripten.sh:
#   - `cargo +nightly` with `-Z build-std=panic_abort,std` + JSON target
#   - `EMCC_CFLAGS=-fwasm-exceptions` env override (forces all cmake
#     sub-targets onto native wasm EH; cmake CMAKE_CXX_FLAGS alone gets
#     dropped by some sub-projects)
#   - emcc post-link adds `-sMEMORY64=1 -sMAXIMUM_MEMORY=16GB
#     -fwasm-exceptions`
#   - Output dir is pkg-bundler-wasm64/, npm package is *-wasm64.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")"/../../.. && pwd)"
JS_DIR="$ROOT/nobodywho/js"
PKG_DIR="$JS_DIR/pkg-bundler-wasm64"
TARGET_SPEC="$JS_DIR/targets/wasm64-unknown-emscripten.json"
TARGET_TRIPLE="wasm64-unknown-emscripten"
TARGET_DIR="$ROOT/nobodywho/target/$TARGET_TRIPLE/release"

EMSDK_DIR="${EMSDK_DIR:-/Users/user/git/emscripten-wbg}"
EM_CONFIG="${EM_CONFIG:-$EMSDK_DIR/.emscripten}"
WASM_BINDGEN_BIN="${EM_WASM_BINDGEN:-/tmp/wbg-patched/bin/wasm-bindgen}"

for f in "$TARGET_SPEC" "$EMSDK_DIR/emcc" "$WASM_BINDGEN_BIN"; do
  [[ -e "$f" ]] || { echo "error: missing $f" >&2; exit 1; }
done

NIGHTLY="$HOME/.rustup/toolchains/nightly-aarch64-apple-darwin"
if [[ ! -d "$NIGHTLY" ]]; then
  # Fall back to whatever nightly rustup resolves to.
  NIGHTLY="$(rustup which --toolchain nightly rustc 2>/dev/null | sed 's|/bin/rustc||')"
fi
if [[ ! -d "$NIGHTLY" ]]; then
  echo "error: nightly Rust toolchain not found. Run: rustup install nightly" >&2
  exit 1
fi

# Sanity-check the rustlib unwind patch. The build will fail with E0425
# on `unwinder_private_data_size` if the wasm64-emscripten arm is
# missing. Detect early.
LIBUNWIND="$NIGHTLY/lib/rustlib/src/rust/library/unwind/src/libunwind.rs"
if [[ -f "$LIBUNWIND" ]] && ! grep -q 'target_arch = "wasm64".*target_os = "emscripten"' "$LIBUNWIND"; then
  echo "error: rustlib libunwind.rs lacks the wasm64-emscripten cfg arm." >&2
  echo "       Add the following 2 lines after the wasm32-emscripten arm in:" >&2
  echo "       $LIBUNWIND" >&2
  echo "" >&2
  echo "       #[cfg(all(target_arch = \"wasm64\", target_os = \"emscripten\"))]" >&2
  echo "       pub const unwinder_private_data_size: usize = 20;" >&2
  echo "" >&2
  echo "       Reference: https://github.com/nobodywho-ooo/rust/commit/1958efbecba5106dfb95d9c93412fd132eab5b76" >&2
  exit 1
fi

# One-time: populate the wasm64 sysroot with wasm-EH library variants.
# Idempotent — embuilder caches and skips up-to-date targets.
WASM64_SYSROOT="$EMSDK_DIR/cache/sysroot/lib/wasm64-emscripten"
if [[ ! -f "$WASM64_SYSROOT/libc++-wasmexcept.a" ]]; then
  echo "==> populating wasm64 sysroot (one-time, ~6 min)"
  PATH="$EMSDK_DIR:/opt/homebrew/bin:$PATH" EM_CONFIG="$EM_CONFIG" \
    "$EMSDK_DIR/embuilder" build SYSTEM --wasm64 -f
fi

mkdir -p "$PKG_DIR"
cp "$JS_DIR/scripts/pre.js" "$PKG_DIR/pre.js"

echo "==> cargo +nightly build --target $TARGET_SPEC -p nobodywho-js"
# EMCC_CFLAGS hammer: ensures every cmake sub-target gets -fwasm-exceptions
# regardless of whether its CMakeLists overrides CMAKE_CXX_FLAGS. Without
# this, some translation units came out as legacy invoke-EH while others
# came out as native wasm-EH, producing an invalid mixed-mode wasm that
# Emscripten's post-link assertion correctly rejects.
(
  cd "$ROOT/nobodywho"
  PATH="$EMSDK_DIR:$NIGHTLY/bin:$HOME/.cargo/bin:/opt/homebrew/bin:$PATH" \
  EM_CONFIG="$EM_CONFIG" \
  EM_WASM_BINDGEN="$WASM_BINDGEN_BIN" \
  LIBCLANG_PATH="${LIBCLANG_PATH:-/Library/Developer/CommandLineTools/usr/lib}" \
  EMCC_CFLAGS="-fwasm-exceptions" \
  EMCC_CXXFLAGS="-fwasm-exceptions" \
  "$NIGHTLY/bin/cargo" build --release \
    --target "$TARGET_SPEC" \
    -Z build-std=panic_abort,std \
    -Zjson-target-spec \
    -p nobodywho-js
)

echo "==> injecting __wasm_bindgen_emscripten_marker custom section"
python3 "$JS_DIR/scripts/inject-emscripten-marker.py" \
  "$TARGET_DIR/nobodywho_js.wasm" \
  "$TARGET_DIR/nobodywho_js.marked.wasm"

echo "==> wasm-bindgen-cli on the marked wasm"
BINDGEN_OUT="$TARGET_DIR/bindgen-out"
rm -rf "$BINDGEN_OUT"
mkdir -p "$BINDGEN_OUT"
"$WASM_BINDGEN_BIN" \
  "$TARGET_DIR/nobodywho_js.marked.wasm" \
  --keep-lld-exports \
  --keep-debug \
  --out-dir "$BINDGEN_OUT"

if [[ ! -f "$BINDGEN_OUT/library_bindgen.js" ]]; then
  echo "error: wasm-bindgen-cli did not produce library_bindgen.js" >&2
  ls -la "$BINDGEN_OUT" >&2
  exit 1
fi

echo "==> copying artifacts into pkg-bundler-wasm64/"
cp "$BINDGEN_OUT/library_bindgen.js" "$PKG_DIR/library_bindgen.js"
WBG_WASM="$(ls "$BINDGEN_OUT"/*.wasm 2>/dev/null | head -n1)"
[[ -n "$WBG_WASM" ]] || { echo "error: wasm-bindgen produced no .wasm" >&2; exit 1; }
cp "$WBG_WASM" "$PKG_DIR/nobodywho_js_bg.wasm"

echo "==> applying sed patches to library_bindgen.js"
# Same patches as the wasm32 build — HEAPU8/HEAP32 typed-array getters,
# Module.X.__wrap routing for classes, __wbg_call_guard var hoist.
/usr/bin/sed -i.bak \
  -e 's/HEAPU80\(\)/HEAPU8/g' \
  -e 's/HEAP320\(\)/HEAP32/g' \
  -e 's/HEAPU8\(\)/HEAPU8/g' \
  -e 's/HEAP32\(\)/HEAP32/g' \
  -E -e 's/(^|[^A-Za-z_])Model\.__wrap/\1Module.Model.__wrap/g' \
     -e 's/(^|[^A-Za-z_])TokenStream\.__wrap/\1Module.TokenStream.__wrap/g' \
     -e 's/(^|[^A-Za-z_])Chat\.__wrap/\1Module.Chat.__wrap/g' \
  "$PKG_DIR/library_bindgen.js"

/usr/bin/sed -i.bak2 \
  's|        function __wbg_call_guard() {$|        var __wbg_terminated_addr; var __wbg_called_abort;\n        function __wbg_call_guard() {|' \
  "$PKG_DIR/library_bindgen.js"

echo "==> appending extraLibraryFuncs.push(...) for \$-prefixed helpers"
FUNCS=$(
  grep -oE "'\\\$[a-zA-Z_0-9]+'" "$PKG_DIR/library_bindgen.js" \
    | grep -v '__deps' \
    | sort -u \
    | tr '\n' ',' \
    | sed 's/,$//'
)
if [[ -n "$FUNCS" ]]; then
  echo "extraLibraryFuncs.push($FUNCS);" >> "$PKG_DIR/library_bindgen.js"
fi
rm -f "$PKG_DIR/library_bindgen.js".bak*

echo "==> em++ --post-link to build nobodywho_js.js (wasm64)"
(
  cd "$PKG_DIR"
  PATH="$EMSDK_DIR:/opt/homebrew/bin:$PATH" \
  EM_CONFIG="$EM_CONFIG" \
  "$EMSDK_DIR/emcc" \
    nobodywho_js_bg.wasm \
    --post-link \
    --js-library library_bindgen.js \
    --pre-js pre.js \
    -sALLOW_MEMORY_GROWTH=1 \
    -sMAXIMUM_MEMORY=16GB \
    -sMODULARIZE=1 \
    -sEXPORT_ES6=1 \
    -sEXPORT_NAME='createNobodyWhoModule' \
    -sEXPORTED_RUNTIME_METHODS=FS,SYSCALLS \
    -sFORCE_FILESYSTEM=1 \
    -sERROR_ON_UNDEFINED_SYMBOLS=0 \
    -sINVOKE_RUN=0 \
    -sMEMORY64=1 \
    -fwasm-exceptions \
    -Wno-undefined \
    -O1 \
    -o nobodywho_js.js
)

cp "$PKG_DIR/nobodywho_js_bg.wasm" "$PKG_DIR/nobodywho_js.wasm"

echo
echo "==> Done. Outputs in $PKG_DIR/:"
ls -lh "$PKG_DIR" | sed 's/^/    /'
echo
echo "Smoke test:"
echo "  PATH=/opt/homebrew/bin:\$PATH node $JS_DIR/scripts/modelpath-smoke.mjs <model.gguf>"
echo "  (with the smoke importing from pkg-bundler-wasm64/, not pkg-bundler/)"
