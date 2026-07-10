#!/bin/bash
# Assemble a dynamic .framework from a Rust cdylib, embedding the sibling ggml/llama
# dylibs (dynamic-link feature) inside the bundle. The binary references them via
# @rpath and gets an @loader_path rpath, so the whole graph resolves once Xcode/
# CocoaPods embeds & signs the framework. Verified on macOS and the iOS simulator.
#
# Usage:
#   make-apple-framework.sh <src_dir> <dylib> <fw_name> <flat|versioned> <out_dir> [ffi_header] [bundle_id]
#     src_dir     dir containing the cdylib AND the libggml*/libllama* dylibs
#     dylib       cdylib filename within src_dir
#     fw_name     framework + module name (e.g. nobodywhoFFI, nobodywho_flutter)
#     layout      flat (iOS/sim/visionOS/watchOS) | versioned (macOS)
#     out_dir     output dir; the framework is created at <out_dir>/<fw_name>.framework
#     ffi_header  optional: path to a uniffi FFI header -> adds Headers/ + a
#                 `framework module <fw_name>` modulemap (needed by Swift SPM;
#                 omit for flutter_rust_bridge / React Native which link directly)
#     bundle_id   optional CFBundleIdentifier (default ooo.nobodywho.<fw_name>)
set -euo pipefail

SRC_DIR=$1; DYLIB=$2; FW_NAME=$3; LAYOUT=$4; OUT_DIR=$5
FFI_HEADER=${6:-}; BUNDLE_ID=${7:-ooo.nobodywho.$FW_NAME}

FW="$OUT_DIR/$FW_NAME.framework"
rm -rf "$FW"
if [ "$LAYOUT" = versioned ]; then
    ROOT="$FW/Versions/A"
    mkdir -p "$ROOT/Resources"
else
    ROOT="$FW"
    mkdir -p "$ROOT"
fi

# main binary
cp -L "$SRC_DIR/$DYLIB" "$ROOT/$FW_NAME"
install_name_tool -id "@rpath/$FW_NAME.framework/$FW_NAME" "$ROOT/$FW_NAME"
install_name_tool -add_rpath "@loader_path" "$ROOT/$FW_NAME" 2>/dev/null || true

# Embedded ggml/llama dylibs (real files; libX.dylib is now unversioned via the
# reset-soversion override, cp -L dereferences any stray symlink to a real object).
for real in "$SRC_DIR"/libggml*.dylib "$SRC_DIR"/libllama*.dylib; do
    [ -e "$real" ] || continue
    name=$(basename "$real")
    cp -L "$real" "$ROOT/$name"
    install_name_tool -add_rpath "@loader_path" "$ROOT/$name" 2>/dev/null || true
done

# optional uniffi FFI module (framework modulemap with umbrella header)
if [ -n "$FFI_HEADER" ]; then
    mkdir -p "$ROOT/Headers" "$ROOT/Modules"
    cp "$FFI_HEADER" "$ROOT/Headers/"
    cat > "$ROOT/Modules/module.modulemap" << EOF
framework module $FW_NAME {
    umbrella header "$(basename "$FFI_HEADER")"
    export *
}
EOF
fi

# A versioned (macOS) framework keeps Info.plist under Resources/ (where the
# Resources symlink points and CFBundle/codesign expect it); a flat (iOS et al.)
# framework keeps it at the bundle root.
if [ "$LAYOUT" = versioned ]; then
    PLIST="$ROOT/Resources/Info.plist"
else
    PLIST="$ROOT/Info.plist"
fi
# MinimumOSVersion must match the slice's real platform minimum — it differs per
# platform (visionOS 1.x, watchOS 10.x, iOS 18.x, …), so read it from the binary's
# LC_BUILD_VERSION instead of hardcoding a value that's wrong for most slices.
MIN_OS=$(vtool -show-build "$ROOT/$FW_NAME" 2>/dev/null | awk '/minos/{print $2; exit}')
: "${MIN_OS:=13.0}"

plutil -create xml1 "$PLIST"
plutil -insert CFBundleExecutable            -string "$FW_NAME"   "$PLIST"
plutil -insert CFBundleIdentifier            -string "$BUNDLE_ID" "$PLIST"
plutil -insert CFBundleInfoDictionaryVersion -string "6.0"        "$PLIST"
plutil -insert CFBundleName                  -string "$FW_NAME"   "$PLIST"
plutil -insert CFBundlePackageType           -string "FMWK"       "$PLIST"
plutil -insert CFBundleVersion               -string "1"          "$PLIST"
plutil -insert MinimumOSVersion              -string "$MIN_OS"    "$PLIST"

if [ "$LAYOUT" = versioned ]; then
    ln -sf A "$FW/Versions/Current"
    ln -sf "Versions/Current/$FW_NAME" "$FW/$FW_NAME"
    if [ -n "$FFI_HEADER" ]; then
        ln -sf Versions/Current/Headers "$FW/Headers"
        ln -sf Versions/Current/Modules "$FW/Modules"
    fi
    ln -sf Versions/Current/Resources "$FW/Resources"
fi

echo "built $FW"
