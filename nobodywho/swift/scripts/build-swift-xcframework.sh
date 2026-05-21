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

echo "Building nobodywho-uniffi for watchOS device arm64 (aarch64-apple-watchos)..."
cargo +nightly build -p nobodywho-uniffi -Z build-std --target aarch64-apple-watchos --release

echo "Building nobodywho-uniffi for watchOS device arm64_32 (arm64_32-apple-watchos)..."
cargo +nightly build -p nobodywho-uniffi -Z build-std --target arm64_32-apple-watchos --release

echo "Building nobodywho-uniffi for watchOS simulator (aarch64-apple-watchos-sim)..."
cargo +nightly build -p nobodywho-uniffi -Z build-std --target aarch64-apple-watchos-sim --release

# Assemble xcframework with headers
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

for dir in ios-device ios-sim macos visionos-device visionos-sim watchos-device watchos-sim; do
    mkdir -p "$TMPDIR/$dir/Headers"
    cp swift/generated/nobodywhoFFI.h "$TMPDIR/$dir/Headers/"
    cp swift/generated/nobodywhoFFI.modulemap "$TMPDIR/$dir/Headers/module.modulemap"
done

cp target/aarch64-apple-ios/release/libnobodywho_uniffi.a "$TMPDIR/ios-device/"
cp target/aarch64-apple-ios-sim/release/libnobodywho_uniffi.a "$TMPDIR/ios-sim/"
cp target/aarch64-apple-darwin/release/libnobodywho_uniffi.a "$TMPDIR/macos/"
cp target/aarch64-apple-visionos/release/libnobodywho_uniffi.a "$TMPDIR/visionos-device/"
cp target/aarch64-apple-visionos-sim/release/libnobodywho_uniffi.a "$TMPDIR/visionos-sim/"
# Combine arm64 and arm64_32 watchOS device libraries into a fat binary.
# arm64_32: Apple Watch Series 4–8, Ultra 1, SE (32-bit pointers)
# arm64: newer watches with true 64-bit support
echo "Creating fat watchOS device library (arm64 + arm64_32)..."
lipo -create \
    target/aarch64-apple-watchos/release/libnobodywho_uniffi.a \
    target/arm64_32-apple-watchos/release/libnobodywho_uniffi.a \
    -output "$TMPDIR/watchos-device/libnobodywho_uniffi.a"
cp target/aarch64-apple-watchos-sim/release/libnobodywho_uniffi.a "$TMPDIR/watchos-sim/"

rm -rf swift/Frameworks/NobodyWhoNative.xcframework
mkdir -p swift/Frameworks

echo "Creating xcframework..."
xcodebuild -create-xcframework \
    -library "$TMPDIR/ios-device/libnobodywho_uniffi.a" -headers "$TMPDIR/ios-device/Headers" \
    -library "$TMPDIR/ios-sim/libnobodywho_uniffi.a" -headers "$TMPDIR/ios-sim/Headers" \
    -library "$TMPDIR/macos/libnobodywho_uniffi.a" -headers "$TMPDIR/macos/Headers" \
    -library "$TMPDIR/visionos-device/libnobodywho_uniffi.a" -headers "$TMPDIR/visionos-device/Headers" \
    -library "$TMPDIR/visionos-sim/libnobodywho_uniffi.a" -headers "$TMPDIR/visionos-sim/Headers" \
    -library "$TMPDIR/watchos-device/libnobodywho_uniffi.a" -headers "$TMPDIR/watchos-device/Headers" \
    -library "$TMPDIR/watchos-sim/libnobodywho_uniffi.a" -headers "$TMPDIR/watchos-sim/Headers" \
    -output swift/Frameworks/NobodyWhoNative.xcframework

echo "Done: swift/Frameworks/NobodyWhoNative.xcframework"
