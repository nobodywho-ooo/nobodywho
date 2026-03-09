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
LIB_NAME="libnobodywho_uniffi.so"

# Resolve NDK path.
NDK="${ANDROID_NDK:-${ANDROID_HOME}/ndk-bundle}"
if [ ! -d "$NDK" ]; then
    echo "Error: Android NDK not found."
    echo "Set ANDROID_NDK to the NDK directory (e.g. ~/Library/Android/sdk/ndk/28.2.13676358)"
    exit 1
fi

# ABI → Rust target triple map.
declare -A TARGETS=(
    ["arm64-v8a"]="aarch64-linux-android"
    ["armeabi-v7a"]="armv7-linux-androideabi"
    ["x86_64"]="x86_64-linux-android"
    ["x86"]="i686-linux-android"
)

# Minimum Android API level for the linker.
API_LEVEL=24

TOOLCHAIN="$NDK/toolchains/llvm/prebuilt"
# Select the host toolchain directory.
if [ -d "$TOOLCHAIN/linux-x86_64" ]; then
    HOST_TAG="linux-x86_64"
elif [ -d "$TOOLCHAIN/darwin-x86_64" ]; then
    HOST_TAG="darwin-x86_64"
elif [ -d "$TOOLCHAIN/darwin-arm64" ]; then
    HOST_TAG="darwin-arm64"
else
    echo "Error: Could not find NDK host toolchain in $TOOLCHAIN"
    exit 1
fi
BIN="$TOOLCHAIN/$HOST_TAG/bin"

echo "Building for all Android ABIs..."
echo "  NDK: $NDK"
echo ""

for ABI in "${!TARGETS[@]}"; do
    TARGET="${TARGETS[$ABI]}"
    echo "── $ABI ($TARGET) ──"

    # Add the Rust target if missing.
    rustup target add "$TARGET" 2>/dev/null || true

    # Resolve the correct clang triple.
    case "$TARGET" in
        armv7-linux-androideabi)
            CLANG_TRIPLE="armv7a-linux-androideabi${API_LEVEL}"
            ;;
        *)
            CLANG_TRIPLE="${TARGET}${API_LEVEL}"
            ;;
    esac

    CLANG="$BIN/${CLANG_TRIPLE}-clang"
    AR="$BIN/llvm-ar"

    TARGET_UPPER="${TARGET//-/_}"
    TARGET_UPPER="${TARGET_UPPER^^}"

    CC_VAR="CC_${TARGET//-/_}"
    AR_VAR="AR_${TARGET//-/_}"
    LINKER_VAR="CARGO_TARGET_${TARGET_UPPER}_LINKER"

    eval "export $CC_VAR=$CLANG"
    eval "export $AR_VAR=$AR"
    eval "export $LINKER_VAR=$CLANG"

    cargo build \
        --release \
        -p nobodywho-uniffi \
        --target "$TARGET" \
        --manifest-path "$ROOT_DIR/Cargo.toml"

    DEST="$JNILIBS_DIR/$ABI"
    mkdir -p "$DEST"
    cp "$ROOT_DIR/target/$TARGET/release/$LIB_NAME" "$DEST/$LIB_NAME"
    echo "  → $DEST/$LIB_NAME"
    echo ""
done

echo "Building AAR..."
cd "$ANDROID_DIR"
./gradlew :library:assembleRelease

AAR_PATH="$ANDROID_DIR/library/build/outputs/aar/library-release.aar"
echo ""
echo "Done: $AAR_PATH"
