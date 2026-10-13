#!/usr/bin/env bash
# Builds ONNX Runtime as an Emscripten static library at <out-dir>/lib/libonnxruntime.a, for NobodyWho to link via ORT_LIB_PATH.
# Run it where `emcc` is on PATH (the default dev shell); ONNX Runtime is built with that same Emscripten version.
set -euo pipefail

ORT_VERSION="${1:?usage: build-ort-wasm.sh <onnxruntime tag> <out-dir>}"
OUT_DIR="$(realpath -m "${2:?usage: build-ort-wasm.sh <onnxruntime tag> <out-dir>}")"
SRC_DIR="$OUT_DIR/src"
BUILD_DIR="$OUT_DIR/build"
LIB="$OUT_DIR/lib/libonnxruntime.a"

if [[ -f "$LIB" ]]; then
    echo "ONNX Runtime $ORT_VERSION for Emscripten is already built: $LIB"
    exit 0
fi

if [[ ! -d "$SRC_DIR" ]]; then
    git clone --recursive --shallow-submodules --depth 1 --branch "$ORT_VERSION" \
        https://github.com/microsoft/onnxruntime "$SRC_DIR"
fi

# build.py always installs its own emsdk, but its LLVM differs from ours and the release build emits LTO bitcode, so we override its toolchain with the shell's.
EMSCRIPTEN_TOOLCHAIN="$(em-config EMSCRIPTEN_ROOT)/cmake/Modules/Platform/Emscripten.cmake"

# Rust links with legacy wasm exceptions (-fwasm-exceptions); ONNX Runtime's own modes use JS exceptions or exnref, so disable those and pass the flag ourselves.
# Warnings aren't errors because bundled abseil uses macros that Emscripten 6 deprecates.
python3 "$SRC_DIR/tools/ci_build/build.py" \
    --build_dir "$BUILD_DIR" \
    --config Release \
    --skip_tests \
    --skip_submodule_sync \
    --parallel \
    --compile_no_warning_as_error \
    --build_wasm_static_lib \
    --disable_wasm_exception_catching \
    --cmake_extra_defines \
        "CMAKE_TOOLCHAIN_FILE=$EMSCRIPTEN_TOOLCHAIN" \
        CMAKE_C_FLAGS=-fwasm-exceptions \
        CMAKE_CXX_FLAGS=-fwasm-exceptions \
        onnxruntime_BUILD_UNIT_TESTS=OFF

mkdir -p "$(dirname "$LIB")"
cp "$BUILD_DIR/Release/libonnxruntime_webassembly.a" "$LIB"
echo "Built $LIB"
