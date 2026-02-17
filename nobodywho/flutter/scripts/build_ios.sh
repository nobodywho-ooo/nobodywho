#!/bin/bash
set -e

# Build nobodywho for iOS development, heavily vibe-coded.
# This script builds all iOS targets and creates the xcframework
# so you can run the example_app on your iPhone or simulator

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
echo "Building nobodywho for iOS"
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

# Ensure iOS targets are installed
echo ""
echo "Checking Rust iOS targets..."
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios 2>/dev/null || true

# Set iOS deployment target to ensure C/C++ dependencies (like llama-cpp-sys)
# are compiled with the correct minimum iOS version. Without this, the build
# may pick up the macOS version instead, causing linker errors.
export IPHONEOS_DEPLOYMENT_TARGET=18.5

if [ "$SKIP_BUILD" = false ]; then
    echo ""
    echo "Step 1/4: Building for iOS device (aarch64-apple-ios)..."
    cargo build -p nobodywho-flutter --target aarch64-apple-ios $CARGO_PROFILE_FLAG --manifest-path "$NOBODYWHO_DIR/Cargo.toml"

    echo ""
    echo "Step 2/4: Building for iOS simulator (aarch64-apple-ios-sim)..."
    cargo build -p nobodywho-flutter --target aarch64-apple-ios-sim $CARGO_PROFILE_FLAG --manifest-path "$NOBODYWHO_DIR/Cargo.toml"

    echo ""
    echo "Step 3/4: Building for iOS simulator (x86_64-apple-ios)..."
    cargo build -p nobodywho-flutter --target x86_64-apple-ios $CARGO_PROFILE_FLAG --manifest-path "$NOBODYWHO_DIR/Cargo.toml"
else
    echo ""
    echo "Skipping cargo build (--skip-build flag)"
fi

echo ""
echo "Step 4/4: Creating XCFramework..."

# Create universal simulator dynamic library
mkdir -p "$TARGET_DIR/universal-ios-sim/$BUILD_TYPE"
lipo -create \
    "$TARGET_DIR/aarch64-apple-ios-sim/$BUILD_TYPE/libnobodywho_flutter.dylib" \
    "$TARGET_DIR/x86_64-apple-ios/$BUILD_TYPE/libnobodywho_flutter.dylib" \
    -output "$TARGET_DIR/universal-ios-sim/$BUILD_TYPE/libnobodywho_flutter.dylib"

# Set install name for iOS simulator
install_name_tool -id @rpath/nobodywho_flutter.framework/nobodywho_flutter \
    "$TARGET_DIR/universal-ios-sim/$BUILD_TYPE/libnobodywho_flutter.dylib"

# Create iOS simulator framework structure
IOS_SIM_FRAMEWORK="$TARGET_DIR/universal-ios-sim/$BUILD_TYPE/nobodywho_flutter.framework"
mkdir -p "$IOS_SIM_FRAMEWORK"
cp "$TARGET_DIR/universal-ios-sim/$BUILD_TYPE/libnobodywho_flutter.dylib" \
    "$IOS_SIM_FRAMEWORK/nobodywho_flutter"

# Create Info.plist for simulator framework
cat > "$IOS_SIM_FRAMEWORK/Info.plist" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>nobodywho_flutter</string>
    <key>CFBundleIdentifier</key>
    <string>ooo.nobodywho.flutter</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>nobodywho_flutter</string>
    <key>CFBundlePackageType</key>
    <string>FMWK</string>
    <key>CFBundleVersion</key>
    <string>1</string>
</dict>
</plist>
EOF

# Set install name for iOS device
install_name_tool -id @rpath/nobodywho_flutter.framework/nobodywho_flutter \
    "$TARGET_DIR/aarch64-apple-ios/$BUILD_TYPE/libnobodywho_flutter.dylib"

# Create iOS device framework structure
IOS_DEVICE_FRAMEWORK="$TARGET_DIR/aarch64-apple-ios/$BUILD_TYPE/nobodywho_flutter.framework"
mkdir -p "$IOS_DEVICE_FRAMEWORK"
cp "$TARGET_DIR/aarch64-apple-ios/$BUILD_TYPE/libnobodywho_flutter.dylib" \
    "$IOS_DEVICE_FRAMEWORK/nobodywho_flutter"

# Create Info.plist for device framework
cat > "$IOS_DEVICE_FRAMEWORK/Info.plist" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>nobodywho_flutter</string>
    <key>CFBundleIdentifier</key>
    <string>ooo.nobodywho.flutter</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>nobodywho_flutter</string>
    <key>CFBundlePackageType</key>
    <string>FMWK</string>
    <key>CFBundleVersion</key>
    <string>1</string>
</dict>
</plist>
EOF

# Clean existing xcframework
rm -rf "$XCFRAMEWORK_OUTPUT"

# Create XCFramework
xcodebuild -create-xcframework \
    -framework "$IOS_DEVICE_FRAMEWORK" \
    -framework "$IOS_SIM_FRAMEWORK" \
    -output "$XCFRAMEWORK_OUTPUT"

echo ""
echo "========================================"
echo "Build complete!"
echo ""
echo "XCFramework created at:"
echo "  $XCFRAMEWORK_OUTPUT"
echo ""
echo "To run the example app:"
echo ""
echo "  export NOBODYWHO_FLUTTER_XCFRAMEWORK=\"$XCFRAMEWORK_OUTPUT\""
echo "  cd $FLUTTER_DIR/example_app"
echo "  flutter run"
echo ""
echo "Or run this one-liner:"
echo ""
echo "  NOBODYWHO_FLUTTER_XCFRAMEWORK=\"$XCFRAMEWORK_OUTPUT\" flutter run"
echo "========================================"
