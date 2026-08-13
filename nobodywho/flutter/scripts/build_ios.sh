#!/bin/bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
TARGET="$ROOT/target"
OUTPUT="$TARGET/xcframework/nobodywho_flutter.xcframework"
PROFILE=release
BUILD=true

for arg in "$@"; do
    case "$arg" in
        --debug) PROFILE=debug ;;
        --skip-build) BUILD=false ;;
        -h|--help)
            echo "Usage: $0 [--debug] [--skip-build]"
            exit
            ;;
        *) echo "Unknown option: $arg" >&2; exit 1 ;;
    esac
done

# Passed as a single always-present flag rather than a possibly-empty array:
# macOS ships bash 3.2, where "${arr[@]}" on an empty array trips `set -u`.
if [ "$PROFILE" = release ]; then CARGO_PROFILE=release; else CARGO_PROFILE=dev; fi
targets=(aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios)
export IPHONEOS_DEPLOYMENT_TARGET=18.5
if $BUILD; then
    rustup target add "${targets[@]}"
    for target in "${targets[@]}"; do
        cargo build -p nobodywho-flutter --manifest-path "$ROOT/Cargo.toml" \
            --target "$target" --profile "$CARGO_PROFILE"
    done
fi

DEVICE="$TARGET/aarch64-apple-ios/$PROFILE"
SIM_ARM="$TARGET/aarch64-apple-ios-sim/$PROFILE"
SIM_X64="$TARGET/x86_64-apple-ios/$PROFILE"
SIM="$TARGET/universal-ios-sim/$PROFILE"
mkdir -p "$SIM"
lipo -create "$SIM_ARM/libnobodywho_flutter.dylib" "$SIM_X64/libnobodywho_flutter.dylib" \
    -output "$SIM/libnobodywho_flutter.dylib"
bash "$ROOT/scripts/lipo-apple-libs.sh" "$SIM_ARM" "$SIM_X64" "$SIM"

SIM_FRAMEWORKS="$SIM/frameworks"
DEVICE_FRAMEWORKS="$DEVICE/frameworks"
rm -rf "$SIM_FRAMEWORKS" "$DEVICE_FRAMEWORKS" "$OUTPUT"
mkdir -p "$SIM_FRAMEWORKS" "$DEVICE_FRAMEWORKS"
make_framework() {
    bash "$ROOT/scripts/make-apple-framework.sh" "$1" \
        libnobodywho_flutter.dylib nobodywho_flutter flat "$2" \
        "" ooo.nobodywho.flutter
}
make_framework "$SIM" "$SIM_FRAMEWORKS"
make_framework "$DEVICE" "$DEVICE_FRAMEWORKS"
xcodebuild -create-xcframework \
    -framework "$DEVICE_FRAMEWORKS/nobodywho_flutter.framework" \
    -framework "$SIM_FRAMEWORKS/nobodywho_flutter.framework" \
    -output "$OUTPUT"

echo "Built $OUTPUT"
echo "Run: flutter run"
