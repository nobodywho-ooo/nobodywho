#!/bin/bash
# Build NobodyWhoNative.xcframework for local Swift development.
# Requires: macOS with Xcode, Rust with iOS targets installed.
#
# Usage (from nobodywho/ workspace root):
#   ./scripts/build-swift-xcframework.sh
#
# After running, Package.swift can resolve the local binary target at:
#   swift/Frameworks/NobodyWhoNative.xcframework

set -euo pipefail
cd "$(dirname "$0")/.."

echo "Building nobodywho-uniffi for iOS device (aarch64-apple-ios)..."
cargo build -p nobodywho-uniffi --target aarch64-apple-ios --release

echo "Building nobodywho-uniffi for iOS simulator (aarch64-apple-ios-sim)..."
cargo build -p nobodywho-uniffi --target aarch64-apple-ios-sim --release

# Assemble xcframework with headers
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

mkdir -p "$TMPDIR/device/Headers" "$TMPDIR/sim/Headers"

cp target/aarch64-apple-ios/release/libnobodywho_uniffi.a "$TMPDIR/device/"
cp target/aarch64-apple-ios-sim/release/libnobodywho_uniffi.a "$TMPDIR/sim/"

for dir in "$TMPDIR/device" "$TMPDIR/sim"; do
    cp swift/generated/nobodywhoFFI.h "$dir/Headers/"
    cp swift/generated/nobodywhoFFI.modulemap "$dir/Headers/module.modulemap"
done

rm -rf swift/Frameworks/NobodyWhoNative.xcframework
mkdir -p swift/Frameworks

echo "Creating xcframework..."
xcodebuild -create-xcframework \
    -library "$TMPDIR/device/libnobodywho_uniffi.a" -headers "$TMPDIR/device/Headers" \
    -library "$TMPDIR/sim/libnobodywho_uniffi.a" -headers "$TMPDIR/sim/Headers" \
    -output swift/Frameworks/NobodyWhoNative.xcframework

echo "Done: swift/Frameworks/NobodyWhoNative.xcframework"
