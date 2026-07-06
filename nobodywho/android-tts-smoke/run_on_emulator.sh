#!/usr/bin/env bash
# End-to-end smoke test: cross-compile, push to emulator, run.
# Requires a running x86_64 Android emulator (any AVD on API 31+).
#
# Usage:
#   ./android-tts-smoke/run_on_emulator.sh
#
# Optional env overrides:
#   ANDROID_NDK   — path to NDK (defaults to ~/Library/Android/sdk/ndk/28.2.13676358)
#   ADB           — path to adb   (defaults to ~/Library/Android/sdk/platform-tools/adb)

set -euo pipefail

export ANDROID_NDK="${ANDROID_NDK:-$HOME/Library/Android/sdk/ndk/28.2.13676358}"
ADB="${ADB:-$HOME/Library/Android/sdk/platform-tools/adb}"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if [[ ! -d "$ANDROID_NDK" ]]; then
    echo "ANDROID_NDK does not exist: $ANDROID_NDK" >&2
    exit 1
fi
if ! "$ADB" devices | grep -q "emulator-\|device$"; then
    echo "No connected adb device. Start an emulator first." >&2
    "$ADB" devices >&2
    exit 1
fi

# Pick the cargo target + NDK ABI to match the connected device.
DEVICE_ABI="$("$ADB" shell getprop ro.product.cpu.abi | tr -d '\r')"
case "$DEVICE_ABI" in
    arm64-v8a)
        TARGET=aarch64-linux-android
        TARGET_PREFIX=aarch64-linux-android31
        TARGET_UPPER=AARCH64_LINUX_ANDROID
        JNI_ABI=arm64-v8a
        ;;
    x86_64)
        TARGET=x86_64-linux-android
        TARGET_PREFIX=x86_64-linux-android31
        TARGET_UPPER=X86_64_LINUX_ANDROID
        JNI_ABI=x86_64
        ;;
    *)
        echo "Unsupported device ABI: $DEVICE_ABI" >&2
        exit 1
        ;;
esac
echo "==> Device ABI: $DEVICE_ABI (target: $TARGET)"

# Cross-compile env
HOST_TAG="darwin-x86_64"   # NDK uses darwin-x86_64 even on Apple Silicon (rosetta or universal binaries)
NDK_BIN="$ANDROID_NDK/toolchains/llvm/prebuilt/$HOST_TAG/bin"
NDK_SYSROOT="$ANDROID_NDK/toolchains/llvm/prebuilt/$HOST_TAG/sysroot"
TARGET_LOWER="$(echo "$TARGET" | tr - _)"
export "CARGO_TARGET_${TARGET_UPPER}_LINKER"="$NDK_BIN/${TARGET_PREFIX}-clang"
export "CC_${TARGET_LOWER}"="$NDK_BIN/${TARGET_PREFIX}-clang"
export "AR_${TARGET_LOWER}"="$NDK_BIN/llvm-ar"
export "CXX_${TARGET_LOWER}"="$NDK_BIN/${TARGET_PREFIX}-clang++"
export "BINDGEN_EXTRA_CLANG_ARGS_${TARGET_LOWER}"="--sysroot=$NDK_SYSROOT --target=$TARGET_PREFIX"

echo "==> Building android-tts-smoke for $TARGET"
cd "$REPO_ROOT"
cargo build -p android-tts-smoke --target "$TARGET" --release

BIN="$REPO_ROOT/target/$TARGET/release/android-tts-smoke"
LIBCXX_SO="$NDK_SYSROOT/usr/lib/$TARGET/libc++_shared.so"
MODEL_DIR="$REPO_ROOT/models/kokoro-v1"
REMOTE=/data/local/tmp

# ort's download-binaries feature fetches libonnxruntime.so into the build
# output dir during cargo build — find it there.
ORT_SO="$(find "$REPO_ROOT/target/$TARGET/release/build" -path "*/ort-sys-*/out/libonnxruntime.so" 2>/dev/null | head -1)"
if [[ -z "$ORT_SO" ]]; then
    echo "libonnxruntime.so not found in cargo build output — did the build succeed?" >&2
    exit 1
fi

# dynamic-link: stage the smoke binary + the ggml/llama .so, then normalize them
# to unversioned SONAMEs (Android needs unversioned; requires patchelf) and repoint
# the binary's DT_NEEDED before pushing. LD_LIBRARY_PATH=. below finds them.
STAGE="$(mktemp -d)"
cp "$BIN" "$STAGE/"
find "$REPO_ROOT/target/$TARGET/release" \
    \( -name 'libggml*.so*' -o -name 'libllama*.so*' \) -exec cp -L -n {} "$STAGE/" \;
bash "$REPO_ROOT/scripts/unversion-elf-libs.sh" "$STAGE" "$STAGE/$(basename "$BIN")"

echo "==> Pushing artifacts to $REMOTE"
"$ADB" push "$STAGE/." "$REMOTE/"
"$ADB" push "$ORT_SO" "$REMOTE/"
"$ADB" push "$LIBCXX_SO" "$REMOTE/"
"$ADB" push "$MODEL_DIR" "$REMOTE/kokoro-v1"
"$ADB" shell chmod +x "$REMOTE/android-tts-smoke"

echo "==> Running synth on emulator"
"$ADB" shell "cd $REMOTE && LD_LIBRARY_PATH=. ./android-tts-smoke ./kokoro-v1 'Hello from Android' out.wav"

echo "==> Pulling result"
"$ADB" pull "$REMOTE/out.wav" /tmp/android-out.wav
file /tmp/android-out.wav
echo "Saved /tmp/android-out.wav — play with: afplay /tmp/android-out.wav"
