#!/bin/bash
set -e

# Build nobodywho_flutter for macOS development, heavily vibe-coded.
# This script builds all macOS targets and creates the xcframework
# so you can run the example_app on macOS

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FLUTTER_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
NOBODYWHO_DIR="$(cd "$FLUTTER_DIR/.." && pwd)"
TARGET_DIR="$NOBODYWHO_DIR/target"
XCFRAMEWORK_OUTPUT="$TARGET_DIR/xcframework/NobodyWhoFlutter.xcframework"

# Parse arguments
BUILD_TYPE="release"
SKIP_BUILD=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --debug)
            BUILD_TYPE="debug"
            shift
            ;;
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --debug       Build debug instead of release"
            echo "  --skip-build  Skip cargo build, only recreate xcframework"
            echo "  -h, --help    Show this help"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

CARGO_PROFILE_FLAG=""
if [ "$BUILD_TYPE" = "release" ]; then
    CARGO_PROFILE_FLAG="--release"
fi

echo "========================================"
echo "Building nobodywho_flutter for macOS"
echo "Build type: $BUILD_TYPE"
echo "========================================"

# Check for required tools
if ! command -v rustup &> /dev/null; then
    echo "Error: rustup not found. Please install Rust: https://rustup.rs"
    exit 1
fi

if ! command -v xcodebuild &> /dev/null; then
    echo "Error: xcodebuild not found. Please install Xcode."
    exit 1
fi

# Ensure macOS targets are installed
echo ""
echo "Checking Rust macOS targets..."
rustup target add aarch64-apple-darwin x86_64-apple-darwin 2>/dev/null || true

if [ "$SKIP_BUILD" = false ]; then
    echo ""
    echo "Step 1/2: Building for macOS (aarch64-apple-darwin)..."
    cargo build -p nobodywho-flutter --target aarch64-apple-darwin $CARGO_PROFILE_FLAG --manifest-path "$NOBODYWHO_DIR/Cargo.toml"

    echo ""
    echo "Step 2/2: Building for macOS (x86_64-apple-darwin)..."
    cargo build -p nobodywho-flutter --target x86_64-apple-darwin $CARGO_PROFILE_FLAG --manifest-path "$NOBODYWHO_DIR/Cargo.toml"
else
    echo ""
    echo "Skipping cargo build (--skip-build flag)"
fi

echo ""
echo "Step 3/3: Creating XCFramework..."

# Create universal macOS library
mkdir -p "$TARGET_DIR/universal-macos/$BUILD_TYPE"
lipo -create \
    "$TARGET_DIR/aarch64-apple-darwin/$BUILD_TYPE/libnobodywho_flutter.a" \
    "$TARGET_DIR/x86_64-apple-darwin/$BUILD_TYPE/libnobodywho_flutter.a" \
    -output "$TARGET_DIR/universal-macos/$BUILD_TYPE/libnobodywho_flutter.a"

# Create headers directory
HEADERS_DIR="$TARGET_DIR/xcframework/headers"
mkdir -p "$HEADERS_DIR"
cp "$FLUTTER_DIR/nobodywho_flutter/ios/Classes/binding.h" "$HEADERS_DIR/"
cat > "$HEADERS_DIR/module.modulemap" << 'EOF'
module CBinding {
    header "binding.h"
    export *
}
EOF

# Clean existing xcframework
rm -rf "$XCFRAMEWORK_OUTPUT"

# Create XCFramework (macOS only)
xcodebuild -create-xcframework \
    -library "$TARGET_DIR/universal-macos/$BUILD_TYPE/libnobodywho_flutter.a" \
    -headers "$HEADERS_DIR" \
    -output "$XCFRAMEWORK_OUTPUT"

echo ""
echo "========================================"
echo "Build complete!"
echo ""
echo "XCFramework created at:"
echo "  $XCFRAMEWORK_OUTPUT"
echo ""
echo "To run the example app on macOS:"
echo ""
echo "  export NOBODYWHO_FLUTTER_XCFRAMEWORK=\"$XCFRAMEWORK_OUTPUT\""
echo "  cd $FLUTTER_DIR/example_app"
echo "  flutter run -d macos"
echo ""
echo "Or run this one-liner:"
echo ""
echo "  NOBODYWHO_FLUTTER_XCFRAMEWORK=\"$XCFRAMEWORK_OUTPUT\" flutter run -d macos"
echo "========================================"

