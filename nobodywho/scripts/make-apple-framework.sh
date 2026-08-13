#!/bin/bash
# Usage: make-apple-framework.sh <src-dir> <dylib> <name> <flat|versioned> <out-dir> [header] [bundle-id]
set -euo pipefail

SRC_DIR=$1; DYLIB=$2; FW_NAME=$3; LAYOUT=$4; OUT_DIR=$5
FFI_HEADER=${6:-}; BUNDLE_ID=${7:-ooo.nobodywho.$FW_NAME}
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
RUNTIME_TOOL="$SCRIPT_DIR/runtime-bundle.py"
RUNTIME_MANIFEST="$SRC_DIR/nobodywho-runtime/manifest.json"

FW="$OUT_DIR/$FW_NAME.framework"
rm -rf "$FW"
if [ "$LAYOUT" = versioned ]; then
    ROOT="$FW/Versions/A"
    mkdir -p "$ROOT/Resources"
else
    ROOT="$FW"
    mkdir -p "$ROOT"
fi

cp -L "$SRC_DIR/$DYLIB" "$ROOT/$FW_NAME"
install_name_tool -id "@rpath/$FW_NAME.framework/$FW_NAME" "$ROOT/$FW_NAME"

python3 "$RUNTIME_TOOL" copy "$RUNTIME_MANIFEST" "$ROOT"

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

if [ "$LAYOUT" = versioned ]; then
    PLIST="$ROOT/Resources/Info.plist"
else
    PLIST="$ROOT/Info.plist"
fi
MIN_OS=$(vtool -show-build "$ROOT/$FW_NAME" | awk '/minos/{print $2; exit}')
if [ -z "$MIN_OS" ]; then
    echo "make-apple-framework: could not read MinimumOSVersion (minos) from $ROOT/$FW_NAME" >&2
    exit 1
fi

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
