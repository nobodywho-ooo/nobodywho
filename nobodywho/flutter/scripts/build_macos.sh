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
if $BUILD; then
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
    for target in aarch64-apple-darwin x86_64-apple-darwin; do
        cargo build -p nobodywho-flutter --manifest-path "$ROOT/Cargo.toml" \
            --target "$target" --profile "$CARGO_PROFILE"
    done
fi

ARM="$TARGET/aarch64-apple-darwin/$PROFILE"
X64="$TARGET/x86_64-apple-darwin/$PROFILE"
UNIVERSAL="$TARGET/universal-macos/$PROFILE"
mkdir -p "$UNIVERSAL"
lipo -create "$ARM/libnobodywho_flutter.dylib" "$X64/libnobodywho_flutter.dylib" \
    -output "$UNIVERSAL/libnobodywho_flutter.dylib"
bash "$ROOT/apple/lipo-runtime.sh" "$ARM" "$X64" "$UNIVERSAL"

FRAMEWORKS="$UNIVERSAL/frameworks"
rm -rf "$FRAMEWORKS" "$OUTPUT"
mkdir -p "$FRAMEWORKS"
bash "$ROOT/apple/make-framework.sh" "$UNIVERSAL" \
    libnobodywho_flutter.dylib nobodywho_flutter versioned "$FRAMEWORKS" \
    "" ooo.nobodywho.flutter
xcodebuild -create-xcframework \
    -framework "$FRAMEWORKS/nobodywho_flutter.framework" \
    -output "$OUTPUT"

echo "Built $OUTPUT"
echo "Run: flutter run -d macos"
