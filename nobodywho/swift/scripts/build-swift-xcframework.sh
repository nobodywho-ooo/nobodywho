#!/bin/bash
# Build NobodyWhoNative.xcframework for local Swift development.
# Requires: macOS with Xcode, Rust with Apple targets installed.
# Requires: Rust nightly with rust-src for visionOS/watchOS (tier 3 targets).
#
# Usage (from nobodywho/ workspace root):
#   ./swift/scripts/build-swift-xcframework.sh
#
# After running, Package.swift can resolve the local binary target at:
#   swift/Frameworks/NobodyWhoNative.xcframework
#
# Produces a DYNAMIC-FRAMEWORK xcframework (since the dynamic-link switch): the
# uniffi .dylib wrapped as a framework with the ggml/llama dylibs embedded inside.
# See make-apple-framework.sh for how the @rpath/@loader_path graph is assembled.

set -euo pipefail
cd "$(dirname "$0")/../.."

# Stable targets
echo "Building nobodywho-uniffi for iOS device (aarch64-apple-ios)..."
cargo build -p nobodywho-uniffi --target aarch64-apple-ios --release

echo "Building nobodywho-uniffi for iOS simulator (aarch64-apple-ios-sim)..."
cargo build -p nobodywho-uniffi --target aarch64-apple-ios-sim --release

echo "Building nobodywho-uniffi for macOS (aarch64-apple-darwin)..."
cargo build -p nobodywho-uniffi --target aarch64-apple-darwin --release

# Tier 3 targets (require nightly + build-std)
echo "Building nobodywho-uniffi for visionOS device (aarch64-apple-visionos)..."
cargo +nightly build -p nobodywho-uniffi -Z build-std --target aarch64-apple-visionos --release

echo "Building nobodywho-uniffi for visionOS simulator (aarch64-apple-visionos-sim)..."
cargo +nightly build -p nobodywho-uniffi -Z build-std --target aarch64-apple-visionos-sim --release

echo "Building nobodywho-uniffi for watchOS device (aarch64-apple-watchos)..."
cargo +nightly build -p nobodywho-uniffi -Z build-std --target aarch64-apple-watchos --release

echo "Building nobodywho-uniffi for watchOS simulator (aarch64-apple-watchos-sim)..."
cargo +nightly build -p nobodywho-uniffi -Z build-std --target aarch64-apple-watchos-sim --release

# The framework module is named `nobodywhoFFI` so the generated
# `nobodywho.swift`'s `import nobodywhoFFI` resolves (validated with
# `swift build --target NobodyWhoGenerated`). The SPM binaryTarget in
# Package.swift stays named NobodyWhoNative — the vended module name is
# independent of the binaryTarget name, so Package.swift needs no change.
FRAMEWORK_NAME=nobodywhoFFI
HELPER="$PWD/scripts/make-apple-framework.sh"
FFI_HEADER="$PWD/swift/generated/nobodywhoFFI.h"
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

# $1 = cargo target triple   $2 = output slice dir   $3 = flat|versioned
make_framework() {
    bash "$HELPER" "target/$1/release" libnobodywho_uniffi.dylib \
        "$FRAMEWORK_NAME" "$3" "$2" "$FFI_HEADER" ooo.nobodywho.ffi
}

make_framework aarch64-apple-ios          "$TMPDIR/ios-device"      flat
make_framework aarch64-apple-ios-sim      "$TMPDIR/ios-sim"         flat
make_framework aarch64-apple-darwin       "$TMPDIR/macos"           versioned
make_framework aarch64-apple-visionos     "$TMPDIR/visionos-device" flat
make_framework aarch64-apple-visionos-sim "$TMPDIR/visionos-sim"    flat
make_framework aarch64-apple-watchos      "$TMPDIR/watchos-device"  flat
make_framework aarch64-apple-watchos-sim  "$TMPDIR/watchos-sim"     flat

rm -rf swift/Frameworks/NobodyWhoNative.xcframework
mkdir -p swift/Frameworks

echo "Creating xcframework..."
xcodebuild -create-xcframework \
    -framework "$TMPDIR/ios-device/$FRAMEWORK_NAME.framework" \
    -framework "$TMPDIR/ios-sim/$FRAMEWORK_NAME.framework" \
    -framework "$TMPDIR/macos/$FRAMEWORK_NAME.framework" \
    -framework "$TMPDIR/visionos-device/$FRAMEWORK_NAME.framework" \
    -framework "$TMPDIR/visionos-sim/$FRAMEWORK_NAME.framework" \
    -framework "$TMPDIR/watchos-device/$FRAMEWORK_NAME.framework" \
    -framework "$TMPDIR/watchos-sim/$FRAMEWORK_NAME.framework" \
    -output swift/Frameworks/NobodyWhoNative.xcframework

echo "Done: swift/Frameworks/NobodyWhoNative.xcframework"
