#!/bin/bash
set -e

# Build XCFramework for NobodyWho Swift SDK
# This script builds the Rust library for iOS and macOS, generates Swift bindings,
# and packages everything into an XCFramework

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SWIFT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKSPACE_DIR="$(cd "$SWIFT_DIR/.." && pwd)"
CORE_DIR="$WORKSPACE_DIR/core"
TARGET_DIR="$WORKSPACE_DIR/target"
XCFRAMEWORK_OUTPUT="$SWIFT_DIR/NobodyWhoFFI.xcframework"

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
echo "Building NobodyWho Swift SDK"
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

# Ensure iOS and macOS targets are installed
echo ""
echo "Checking Rust targets..."
rustup target add \
    aarch64-apple-ios \
    aarch64-apple-ios-sim \
    x86_64-apple-ios \
    aarch64-apple-darwin \
    x86_64-apple-darwin \
    2>/dev/null || true

# Set iOS deployment target
export IPHONEOS_DEPLOYMENT_TARGET=13.0
export MACOSX_DEPLOYMENT_TARGET=11.0

if [ "$SKIP_BUILD" = false ]; then
    echo ""
    echo "Building for all Apple targets..."

    # Build for iOS device (arm64)
    echo "  [1/5] iOS device (aarch64-apple-ios)..."
    cargo build -p nobodywho --features uniffi --target aarch64-apple-ios $CARGO_PROFILE_FLAG --manifest-path "$WORKSPACE_DIR/Cargo.toml"

    # Build for iOS simulator (arm64)
    echo "  [2/5] iOS simulator arm64 (aarch64-apple-ios-sim)..."
    cargo build -p nobodywho --features uniffi --target aarch64-apple-ios-sim $CARGO_PROFILE_FLAG --manifest-path "$WORKSPACE_DIR/Cargo.toml"

    # Build for iOS simulator (x86_64)
    echo "  [3/5] iOS simulator x86_64 (x86_64-apple-ios)..."
    cargo build -p nobodywho --features uniffi --target x86_64-apple-ios $CARGO_PROFILE_FLAG --manifest-path "$WORKSPACE_DIR/Cargo.toml"

    # Build for macOS (arm64)
    echo "  [4/5] macOS arm64 (aarch64-apple-darwin)..."
    cargo build -p nobodywho --features uniffi --target aarch64-apple-darwin $CARGO_PROFILE_FLAG --manifest-path "$WORKSPACE_DIR/Cargo.toml"

    # Build for macOS (x86_64)
    echo "  [5/5] macOS x86_64 (x86_64-apple-darwin)..."
    cargo build -p nobodywho --features uniffi --target x86_64-apple-darwin $CARGO_PROFILE_FLAG --manifest-path "$WORKSPACE_DIR/Cargo.toml"
else
    echo ""
    echo "Skipping cargo build (--skip-build flag)"
fi

echo ""
echo "Generating Swift bindings..."

# Generate Swift bindings using uniffi
# We'll use cargo run with the uniffi CLI to generate bindings
cd "$CORE_DIR"
cargo run --features uniffi/cli --package nobodywho --bin uniffi-bindgen -- generate \
    --library "$TARGET_DIR/aarch64-apple-darwin/$BUILD_TYPE/libnobodywho.dylib" \
    --language swift \
    --out-dir "$SWIFT_DIR/Sources/NobodyWho/Generated" \
    src/nobodywho.udl 2>/dev/null || {

    # If uniffi-bindgen binary doesn't exist, use the library approach
    echo "Generating bindings using uniffi library..."

    # Create a temporary directory for generated files
    mkdir -p "$SWIFT_DIR/Sources/NobodyWho/Generated"

    # For now, we'll note that bindings generation will happen during Xcode build
    # The proper way is to integrate uniffi-bindgen into the build process
    echo "Note: Swift bindings will be generated during Xcode build"
    echo "      Place generated .swift files in Sources/NobodyWho/Generated/"
}

cd "$WORKSPACE_DIR"

echo ""
echo "Creating frameworks..."

# Create universal simulator library (iOS)
mkdir -p "$TARGET_DIR/universal-ios-sim/$BUILD_TYPE"
lipo -create \
    "$TARGET_DIR/aarch64-apple-ios-sim/$BUILD_TYPE/libnobodywho.dylib" \
    "$TARGET_DIR/x86_64-apple-ios/$BUILD_TYPE/libnobodywho.dylib" \
    -output "$TARGET_DIR/universal-ios-sim/$BUILD_TYPE/libnobodywho.dylib"

# Create universal macOS library
mkdir -p "$TARGET_DIR/universal-macos/$BUILD_TYPE"
lipo -create \
    "$TARGET_DIR/aarch64-apple-darwin/$BUILD_TYPE/libnobodywho.dylib" \
    "$TARGET_DIR/x86_64-apple-darwin/$BUILD_TYPE/libnobodywho.dylib" \
    -output "$TARGET_DIR/universal-macos/$BUILD_TYPE/libnobodywho.dylib"

# Helper function to create framework structure
create_framework() {
    local FRAMEWORK_DIR="$1"
    local DYLIB_PATH="$2"
    local PLATFORM="$3"

    mkdir -p "$FRAMEWORK_DIR"

    if [ "$PLATFORM" = "macos" ]; then
        # macOS uses versioned framework structure
        mkdir -p "$FRAMEWORK_DIR/Versions/A/Resources"
        mkdir -p "$FRAMEWORK_DIR/Versions/A/Headers"
        mkdir -p "$FRAMEWORK_DIR/Versions/A/Modules"
        cp "$DYLIB_PATH" "$FRAMEWORK_DIR/Versions/A/NobodyWhoFFI"
        install_name_tool -id @rpath/NobodyWhoFFI.framework/NobodyWhoFFI "$FRAMEWORK_DIR/Versions/A/NobodyWhoFFI"

        # Copy headers and modulemap if they exist
        if [ -f "$SWIFT_DIR/Sources/NobodyWho/Generated/NobodyWhoFFIFFI.h" ]; then
            cp "$SWIFT_DIR/Sources/NobodyWho/Generated/NobodyWhoFFIFFI.h" "$FRAMEWORK_DIR/Versions/A/Headers/"
        fi
        if [ -f "$SWIFT_DIR/Sources/NobodyWho/Generated/NobodyWhoFFIFFI.modulemap" ]; then
            cp "$SWIFT_DIR/Sources/NobodyWho/Generated/NobodyWhoFFIFFI.modulemap" "$FRAMEWORK_DIR/Versions/A/Modules/module.modulemap"
        fi

        # Create symlinks
        ln -sf A "$FRAMEWORK_DIR/Versions/Current"
        ln -sf Versions/Current/NobodyWhoFFI "$FRAMEWORK_DIR/NobodyWhoFFI"
        ln -sf Versions/Current/Resources "$FRAMEWORK_DIR/Resources"
        ln -sf Versions/Current/Headers "$FRAMEWORK_DIR/Headers"
        ln -sf Versions/Current/Modules "$FRAMEWORK_DIR/Modules"

        INFO_PLIST="$FRAMEWORK_DIR/Versions/A/Resources/Info.plist"
    else
        # iOS uses flat framework structure
        mkdir -p "$FRAMEWORK_DIR/Headers"
        mkdir -p "$FRAMEWORK_DIR/Modules"
        cp "$DYLIB_PATH" "$FRAMEWORK_DIR/NobodyWhoFFI"
        install_name_tool -id @rpath/NobodyWhoFFI.framework/NobodyWhoFFI "$FRAMEWORK_DIR/NobodyWhoFFI"

        # Copy headers and modulemap if they exist
        if [ -f "$SWIFT_DIR/Sources/NobodyWho/Generated/NobodyWhoFFIFFI.h" ]; then
            cp "$SWIFT_DIR/Sources/NobodyWho/Generated/NobodyWhoFFIFFI.h" "$FRAMEWORK_DIR/Headers/"
        fi
        if [ -f "$SWIFT_DIR/Sources/NobodyWho/Generated/NobodyWhoFFIFFI.modulemap" ]; then
            cp "$SWIFT_DIR/Sources/NobodyWho/Generated/NobodyWhoFFIFFI.modulemap" "$FRAMEWORK_DIR/Modules/module.modulemap"
        fi

        INFO_PLIST="$FRAMEWORK_DIR/Info.plist"
    fi

    # Create Info.plist
    cat > "$INFO_PLIST" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>NobodyWhoFFI</string>
    <key>CFBundleIdentifier</key>
    <string>ooo.nobodywho.ffi</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>NobodyWhoFFI</string>
    <key>CFBundlePackageType</key>
    <string>FMWK</string>
    <key>CFBundleVersion</key>
    <string>1</string>
</dict>
</plist>
EOF
}

# Create frameworks
echo "  Creating iOS device framework..."
IOS_DEVICE_FRAMEWORK="$TARGET_DIR/aarch64-apple-ios/$BUILD_TYPE/NobodyWhoFFI.framework"
create_framework "$IOS_DEVICE_FRAMEWORK" "$TARGET_DIR/aarch64-apple-ios/$BUILD_TYPE/libnobodywho.dylib" "ios"

echo "  Creating iOS simulator framework..."
IOS_SIM_FRAMEWORK="$TARGET_DIR/universal-ios-sim/$BUILD_TYPE/NobodyWhoFFI.framework"
create_framework "$IOS_SIM_FRAMEWORK" "$TARGET_DIR/universal-ios-sim/$BUILD_TYPE/libnobodywho.dylib" "ios"

echo "  Creating macOS framework..."
MACOS_FRAMEWORK="$TARGET_DIR/universal-macos/$BUILD_TYPE/NobodyWhoFFI.framework"
create_framework "$MACOS_FRAMEWORK" "$TARGET_DIR/universal-macos/$BUILD_TYPE/libnobodywho.dylib" "macos"

echo ""
echo "Creating XCFramework..."

# Clean existing xcframework
rm -rf "$XCFRAMEWORK_OUTPUT"

# Create XCFramework
xcodebuild -create-xcframework \
    -framework "$IOS_DEVICE_FRAMEWORK" \
    -framework "$IOS_SIM_FRAMEWORK" \
    -framework "$MACOS_FRAMEWORK" \
    -output "$XCFRAMEWORK_OUTPUT"

echo ""
echo "========================================"
echo "Build complete!"
echo ""
echo "XCFramework created at:"
echo "  $XCFRAMEWORK_OUTPUT"
echo ""
echo "To use in your Swift project:"
echo "  1. Add swift/ directory as a local Swift package"
echo "  2. Or copy $XCFRAMEWORK_OUTPUT to your project"
echo "========================================"
