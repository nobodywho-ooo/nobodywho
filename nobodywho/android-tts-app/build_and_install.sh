#!/usr/bin/env bash
# Build the Rust JNI lib for arm64-v8a, copy the three .so files into the
# Android project's jniLibs/, then build & install the APK on a running emulator.
set -euo pipefail

export ANDROID_NDK="${ANDROID_NDK:-$HOME/Library/Android/sdk/ndk/28.2.13676358}"
ADB="${ADB:-$HOME/Library/Android/sdk/platform-tools/adb}"
APP_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$APP_DIR/.." && pwd)"

if ! "$ADB" devices | grep -q "emulator-\|device$"; then
    echo "No connected adb device. Start an emulator first." >&2
    exit 1
fi

DEVICE_ABI="$("$ADB" shell getprop ro.product.cpu.abi | tr -d '\r')"
if [[ "$DEVICE_ABI" != "arm64-v8a" ]]; then
    echo "This script assumes arm64-v8a (your device is $DEVICE_ABI)." >&2
    exit 1
fi

# NDK ships darwin-x86_64 universal binaries that work on Apple Silicon via Rosetta.
HOST_TAG="${HOST_TAG:-$(uname -s | tr '[:upper:]' '[:lower:]')-x86_64}"
NDK_BIN="$ANDROID_NDK/toolchains/llvm/prebuilt/$HOST_TAG/bin"
NDK_SYSROOT="$ANDROID_NDK/toolchains/llvm/prebuilt/$HOST_TAG/sysroot"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$NDK_BIN/aarch64-linux-android31-clang"
export CC_aarch64_linux_android="$NDK_BIN/aarch64-linux-android31-clang"
export AR_aarch64_linux_android="$NDK_BIN/llvm-ar"
export CXX_aarch64_linux_android="$NDK_BIN/aarch64-linux-android31-clang++"
export BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android="--sysroot=$NDK_SYSROOT --target=aarch64-linux-android31"

echo "==> Building Rust JNI lib for aarch64-linux-android"
cd "$APP_DIR/rust"
cargo build --target aarch64-linux-android --release

echo "==> Staging .so files into app/src/main/jniLibs/arm64-v8a/"
JNI_LIBS="$APP_DIR/app/src/main/jniLibs/arm64-v8a"
mkdir -p "$JNI_LIBS"
cp "$APP_DIR/rust/target/aarch64-linux-android/release/libnobodywho_tts.so" "$JNI_LIBS/"
cp "$NDK_SYSROOT/usr/lib/aarch64-linux-android/libc++_shared.so"          "$JNI_LIBS/"
# dynamic-link: the ggml/llama .so are built as siblings of the JNI lib; the
# Android loader resolves them from jniLibs at runtime.
cp -P "$APP_DIR/rust/target/aarch64-linux-android/release/"libggml*.so "$JNI_LIBS/" 2>/dev/null || true
cp -P "$APP_DIR/rust/target/aarch64-linux-android/release/"libllama*.so "$JNI_LIBS/" 2>/dev/null || true
ls -lh "$JNI_LIBS/"

echo "==> Building APK"
cd "$APP_DIR"
# Use JAVA_HOME if already set; otherwise try Android Studio's bundled JBR (macOS default).
export JAVA_HOME="${JAVA_HOME:-/Applications/Android Studio.app/Contents/jbr/Contents/Home}"
./gradlew :app:assembleDebug

APK="$APP_DIR/app/build/outputs/apk/debug/app-debug.apk"
if [[ ! -f "$APK" ]]; then
    echo "APK missing: $APK" >&2
    exit 1
fi

echo "==> Installing APK"
"$ADB" install -r "$APK"

echo "==> Launching MainActivity"
"$ADB" shell am start -n dev.nobodywho.ttsdemo/.MainActivity
echo "Done. The app downloads the Kokoro model from HuggingFace on first launch."
