#!/bin/bash
set -e

# Build nobodywho-stt for all Apple platforms (iOS device + iOS simulator + macOS universal)
# and produce a single nobodywho_stt.xcframework. Mirrors the pattern used for the main
# nobodywho_flutter framework.
#
# The stt cdylib ships as a separately-loaded dynamic framework so whisper's bundled ggml
# stays in its own dyld two-level namespace and never collides with llama's ggml inside
# the host binary.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FLUTTER_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
NOBODYWHO_DIR="$(cd "$FLUTTER_DIR/.." && pwd)"
TARGET_DIR="$NOBODYWHO_DIR/target"
XCFRAMEWORK_OUTPUT="$TARGET_DIR/xcframework/nobodywho_stt.xcframework"

BUILD_TYPE="release"
SKIP_BUILD=false
PLATFORMS="all"  # all | ios | macos

while [[ $# -gt 0 ]]; do
    case $1 in
        --debug)         BUILD_TYPE="debug"; shift ;;
        --skip-build)    SKIP_BUILD=true; shift ;;
        --ios-only)      PLATFORMS="ios"; shift ;;
        --macos-only)    PLATFORMS="macos"; shift ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --debug         Build debug instead of release"
            echo "  --skip-build    Skip cargo build, only recreate xcframework"
            echo "  --ios-only      Only iOS device + simulator slices"
            echo "  --macos-only    Only macOS universal slice"
            echo "  -h, --help      Show this help"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"; exit 1 ;;
    esac
done

CARGO_PROFILE_FLAG=""
[ "$BUILD_TYPE" = "release" ] && CARGO_PROFILE_FLAG="--release"

echo "========================================"
echo "Building nobodywho-stt ($PLATFORMS, $BUILD_TYPE)"
echo "========================================"

command -v rustup >/dev/null    || { echo "Error: rustup not found"; exit 1; }
command -v xcodebuild >/dev/null || { echo "Error: xcodebuild not found"; exit 1; }

build_ios=false
build_macos=false
case "$PLATFORMS" in
    all)        build_ios=true; build_macos=true ;;
    ios)        build_ios=true ;;
    macos)      build_macos=true ;;
esac

if $build_ios; then
    rustup target add aarch64-apple-ios aarch64-apple-ios-sim 2>/dev/null || true
fi
if $build_macos; then
    rustup target add aarch64-apple-darwin x86_64-apple-darwin 2>/dev/null || true
fi

# Match the deployment target used by build_ios.sh so dylibs are link-compatible
# with the main nobodywho_flutter framework when both are embedded together.
export IPHONEOS_DEPLOYMENT_TARGET=18.5

if [ "$SKIP_BUILD" = false ]; then
    if $build_ios; then
        echo ""
        echo "[iOS device]"
        cargo build -p nobodywho-stt --target aarch64-apple-ios $CARGO_PROFILE_FLAG --manifest-path "$NOBODYWHO_DIR/Cargo.toml"
        echo ""
        echo "[iOS simulator]"
        cargo build -p nobodywho-stt --target aarch64-apple-ios-sim $CARGO_PROFILE_FLAG --manifest-path "$NOBODYWHO_DIR/Cargo.toml"
    fi
    if $build_macos; then
        echo ""
        echo "[macOS arm64]"
        cargo build -p nobodywho-stt --target aarch64-apple-darwin $CARGO_PROFILE_FLAG --manifest-path "$NOBODYWHO_DIR/Cargo.toml"
        echo ""
        echo "[macOS x86_64]"
        cargo build -p nobodywho-stt --target x86_64-apple-darwin $CARGO_PROFILE_FLAG --manifest-path "$NOBODYWHO_DIR/Cargo.toml"
    fi
else
    echo ""
    echo "Skipping cargo build (--skip-build)"
fi

echo ""
echo "Wrapping frameworks..."

# Common Info.plist for the framework bundle. Used as-is for iOS (flat layout)
# and copied into Versions/A/Resources/ for macOS (versioned layout).
INFO_PLIST_TEMPLATE='<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>nobodywho_stt</string>
    <key>CFBundleIdentifier</key>
    <string>ooo.nobodywho.stt</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>nobodywho_stt</string>
    <key>CFBundlePackageType</key>
    <string>FMWK</string>
    <key>CFBundleVersion</key>
    <string>1</string>
</dict>
</plist>'

CREATE_ARGS=()

wrap_ios_framework() {
    local triple="$1"
    local fw_dir="$TARGET_DIR/$triple/$BUILD_TYPE/nobodywho_stt.framework"
    rm -rf "$fw_dir"
    mkdir -p "$fw_dir"
    cp "$TARGET_DIR/$triple/$BUILD_TYPE/libnobodywho_stt.dylib" "$fw_dir/nobodywho_stt"
    install_name_tool -id @rpath/nobodywho_stt.framework/nobodywho_stt "$fw_dir/nobodywho_stt"
    echo "$INFO_PLIST_TEMPLATE" > "$fw_dir/Info.plist"
    CREATE_ARGS+=(-framework "$fw_dir")
}

wrap_macos_framework() {
    local universal_dir="$TARGET_DIR/universal-macos/$BUILD_TYPE"
    local fw_dir="$universal_dir/nobodywho_stt.framework"
    mkdir -p "$universal_dir"
    lipo -create \
        "$TARGET_DIR/aarch64-apple-darwin/$BUILD_TYPE/libnobodywho_stt.dylib" \
        "$TARGET_DIR/x86_64-apple-darwin/$BUILD_TYPE/libnobodywho_stt.dylib" \
        -output "$universal_dir/libnobodywho_stt.dylib"
    install_name_tool -id @rpath/nobodywho_stt.framework/nobodywho_stt "$universal_dir/libnobodywho_stt.dylib"

    rm -rf "$fw_dir"
    mkdir -p "$fw_dir/Versions/A/Resources"
    cp "$universal_dir/libnobodywho_stt.dylib" "$fw_dir/Versions/A/nobodywho_stt"
    echo "$INFO_PLIST_TEMPLATE" > "$fw_dir/Versions/A/Resources/Info.plist"
    ln -sf A                                      "$fw_dir/Versions/Current"
    ln -sf Versions/Current/nobodywho_stt         "$fw_dir/nobodywho_stt"
    ln -sf Versions/Current/Resources             "$fw_dir/Resources"
    CREATE_ARGS+=(-framework "$fw_dir")
}

if $build_ios; then
    wrap_ios_framework aarch64-apple-ios
    wrap_ios_framework aarch64-apple-ios-sim
fi
if $build_macos; then
    wrap_macos_framework
fi

rm -rf "$XCFRAMEWORK_OUTPUT"
mkdir -p "$(dirname "$XCFRAMEWORK_OUTPUT")"
xcodebuild -create-xcframework "${CREATE_ARGS[@]}" -output "$XCFRAMEWORK_OUTPUT"

echo ""
echo "========================================"
echo "Built: $XCFRAMEWORK_OUTPUT"
echo "========================================"
