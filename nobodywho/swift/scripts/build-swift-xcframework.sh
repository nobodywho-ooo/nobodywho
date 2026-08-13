#!/bin/bash
# Requires Xcode and Rust nightly with rust-src.

set -euo pipefail
cd "$(dirname "$0")/../.."

for target in aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin; do
    cargo build -p nobodywho-uniffi --target "$target" --release
done

for target in aarch64-apple-visionos aarch64-apple-visionos-sim aarch64-apple-watchos-sim; do
    cargo +nightly build -p nobodywho-uniffi -Z build-std --target "$target" --release
done

# The built-in watchOS device target disables dynamic libraries.
WATCHOS_SPEC_DIR=$(mktemp -d)
rustc +nightly -Z unstable-options --target aarch64-apple-watchos --print target-spec-json \
  | python3 -c 'import json,sys; d=json.load(sys.stdin); d["dynamic-linking"]=True; d.pop("metadata",None); json.dump(d, open(sys.argv[1],"w"))' \
    "$WATCHOS_SPEC_DIR/aarch64-apple-watchos.json"
cargo +nightly build -p nobodywho-uniffi -Z build-std -Z json-target-spec \
  --target "$WATCHOS_SPEC_DIR/aarch64-apple-watchos.json" --release
rm -rf "$WATCHOS_SPEC_DIR"

FRAMEWORK_NAME=nobodywhoFFI
HELPER="$PWD/scripts/make-apple-framework.sh"
FFI_HEADER="$PWD/swift/generated/nobodywhoFFI.h"
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

frameworks=()
make_framework() {
    local output="$TMPDIR/$3"
    bash "$HELPER" "target/$1/release" libnobodywho_uniffi.dylib \
        "$FRAMEWORK_NAME" "$2" "$output" "$FFI_HEADER" ooo.nobodywho.ffi
    frameworks+=(-framework "$output/$FRAMEWORK_NAME.framework")
}

make_framework aarch64-apple-ios          flat      ios-device
make_framework aarch64-apple-ios-sim      flat      ios-sim
make_framework aarch64-apple-darwin       versioned macos
make_framework aarch64-apple-visionos     flat      visionos-device
make_framework aarch64-apple-visionos-sim flat      visionos-sim
make_framework aarch64-apple-watchos      flat      watchos-device
make_framework aarch64-apple-watchos-sim  flat      watchos-sim

rm -rf swift/Frameworks/NobodyWhoNative.xcframework
mkdir -p swift/Frameworks

xcodebuild -create-xcframework "${frameworks[@]}" \
    -output swift/Frameworks/NobodyWhoNative.xcframework

echo "Done: swift/Frameworks/NobodyWhoNative.xcframework"
