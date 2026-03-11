#!/bin/bash
set -e

# Build the NobodyWho Android AAR.
#
# Prerequisites:
#   - Android NDK installed (set ANDROID_NDK or ANDROID_HOME)
#   - rustup targets added (see below)
#   - uniffi-bindgen Kotlin bindings generated (run generate_bindings.sh first)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ANDROID_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$(cd "$ANDROID_DIR/.." && pwd)"
JNILIBS_DIR="$ANDROID_DIR/library/src/main/jniLibs"
# Cargo builds the crate as libnobodywho_uniffi.so but UniFFI bindings expect
# libuniffi_nobodywho.so (derived from the UDL namespace "nobodywho").
CARGO_LIB_NAME="libnobodywho_uniffi.so"
LIB_NAME="libuniffi_nobodywho.so"

# Resolve NDK path.
NDK="${ANDROID_NDK:-${ANDROID_HOME}/ndk-bundle}"
if [ ! -d "$NDK" ]; then
    echo "Error: Android NDK not found."
    echo "Set ANDROID_NDK to the NDK directory (e.g. ~/Library/Android/sdk/ndk/28.2.13676358)"
    exit 1
fi

# ABI → Rust target triple map.
# Note: x86 (i686), armeabi-v7a, and x86_64 are excluded on Windows — all
# trigger build failures in llama-cpp-sys (cmake assertion / CARGO_CFG_TARGET_FEATURE).
# arm64-v8a covers all modern physical devices.
declare -A TARGETS=(
    ["arm64-v8a"]="aarch64-linux-android"
)

# Minimum Android API level for the linker.
API_LEVEL=24

export ANDROID_NDK_HOME="$NDK"

# Prefer the Android SDK cmake over any system cmake (e.g. MinGW cmake 4.x which
# breaks the Android NDK toolchain). cmake 3.22.1 is known to work.
SDK_CMAKE="$ANDROID_HOME/cmake/3.22.1/bin"
if [ -d "$SDK_CMAKE" ]; then
    export PATH="$SDK_CMAKE:$PATH"
    # Set CMAKE env var so the cmake Rust crate uses this binary directly.
    # Convert to Windows path format (C:\...) for cargo/rustc on Windows.
    WIN_CMAKE=$(cygpath -w "$SDK_CMAKE/cmake.exe" 2>/dev/null || echo "$SDK_CMAKE/cmake.exe")
    export CMAKE="$WIN_CMAKE"
    # Force Ninja generator — cmake 3.22.1 has Ninja bundled and the Android
    # NDK toolchain requires it. Without this cmake defaults to Visual Studio.
    export CMAKE_GENERATOR="Ninja"
    echo "  cmake: $CMAKE (generator: Ninja)"
fi

echo "Building for all Android ABIs..."
echo "  NDK: $NDK"
echo ""

for ABI in "${!TARGETS[@]}"; do
    TARGET="${TARGETS[$ABI]}"
    echo "── $ABI ($TARGET) ──"

    # Add the Rust target if missing.
    rustup target add "$TARGET" 2>/dev/null || true

    cargo ndk \
        --target "$ABI" \
        --platform "$API_LEVEL" \
        --manifest-path "$ROOT_DIR/Cargo.toml" \
        -- build \
        --release \
        -p nobodywho-uniffi

    DEST="$JNILIBS_DIR/$ABI"
    mkdir -p "$DEST"
    cp "$ROOT_DIR/target/$TARGET/release/$CARGO_LIB_NAME" "$DEST/$LIB_NAME"
    echo "  → $DEST/$LIB_NAME"
    echo ""
done

echo "Building AAR..."
cd "$ANDROID_DIR"
export JAVA_HOME="${JAVA_HOME:-C:/Program Files/Android/Android Studio/jbr}"
if [ -f "./gradlew.bat" ]; then
    ./gradlew.bat :library:assembleRelease
else
    ./gradlew :library:assembleRelease
fi

AAR_PATH="$ANDROID_DIR/library/build/outputs/aar/library-release.aar"
echo ""
echo "Done: $AAR_PATH"
