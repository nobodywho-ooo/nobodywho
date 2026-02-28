#!/bin/bash
# Binary resolution script for NobodyWho Swift SDK
# Resolves the XCFramework using multiple strategies:
# 1. Environment variable override
# 2. Local build
# 3. Cached download
# 4. Download from GitHub releases

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SWIFT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKSPACE_DIR="$(cd "$SWIFT_DIR/.." && pwd)"
TARGET_DIR="$WORKSPACE_DIR/target"
XCFRAMEWORK_OUTPUT="$SWIFT_DIR/NobodyWhoFFI.xcframework"
CACHE_DIR="${SWIFT_DIR}/.build/nobodywho_cache"

# Get version from Package.swift or use latest
VERSION=$(grep -o 'version.*=.*"[^"]*"' "$SWIFT_DIR/Package.swift" 2>/dev/null | head -1 | sed 's/.*"\(.*\)".*/\1/' || echo "latest")

BUILD_TYPE="${BUILD_TYPE:-release}"

echo "NobodyWho Swift SDK - Framework Setup"
echo "======================================"

# Strategy 1: Check environment variable
if [ -n "$NOBODYWHO_SWIFT_XCFRAMEWORK_PATH" ]; then
    if [ -d "$NOBODYWHO_SWIFT_XCFRAMEWORK_PATH" ]; then
        echo "✓ Using XCFramework from environment variable: $NOBODYWHO_SWIFT_XCFRAMEWORK_PATH"

        # Create symlink if different from expected location
        if [ "$NOBODYWHO_SWIFT_XCFRAMEWORK_PATH" != "$XCFRAMEWORK_OUTPUT" ]; then
            rm -rf "$XCFRAMEWORK_OUTPUT"
            ln -sf "$NOBODYWHO_SWIFT_XCFRAMEWORK_PATH" "$XCFRAMEWORK_OUTPUT"
        fi
        exit 0
    else
        echo "❌ Error: NOBODYWHO_SWIFT_XCFRAMEWORK_PATH is set but path does not exist: $NOBODYWHO_SWIFT_XCFRAMEWORK_PATH"
        exit 1
    fi
fi

# Strategy 2: Check if already exists locally
if [ -d "$XCFRAMEWORK_OUTPUT" ]; then
    echo "✓ XCFramework already exists at: $XCFRAMEWORK_OUTPUT"
    exit 0
fi

# Strategy 3: Check for local Rust build
if [ -d "$TARGET_DIR" ]; then
    echo "Checking for local Rust build..."

    TARGETS=(
        "aarch64-apple-darwin"
        "x86_64-apple-darwin"
        "aarch64-apple-ios"
        "aarch64-apple-ios-sim"
        "x86_64-apple-ios"
    )

    FOUND_COUNT=0
    for target in "${TARGETS[@]}"; do
        DYLIB_PATH="$TARGET_DIR/$target/$BUILD_TYPE/libnobodywho.dylib"
        if [ -f "$DYLIB_PATH" ]; then
            ((FOUND_COUNT++))
        fi
    done

    if [ $FOUND_COUNT -gt 0 ]; then
        echo "Found local .dylib files ($FOUND_COUNT targets)"
        echo "Building XCFramework from local build..."
        "$SCRIPT_DIR/build_xcframework.sh" --skip-build
        exit 0
    fi
fi

# Strategy 4: Check cache
get_version() {
    # Try to get version from git tag
    if command -v git >/dev/null 2>&1 && [ -d "$WORKSPACE_DIR/.git" ]; then
        local tag=$(git -C "$WORKSPACE_DIR" describe --tags --exact-match 2>/dev/null || echo "")
        if [ -n "$tag" ]; then
            echo "$tag" | sed 's/^v//'
            return
        fi
    fi

    # Fallback to checking releases API for latest
    echo "latest"
}

VERSION=$(get_version)
CACHE_PATH="$CACHE_DIR/$VERSION/NobodyWhoFFI.xcframework"

if [ -d "$CACHE_PATH" ]; then
    echo "✓ Using cached XCFramework: $CACHE_PATH"
    cp -R "$CACHE_PATH" "$XCFRAMEWORK_OUTPUT"
    exit 0
fi

# Strategy 5: Download from GitHub releases
echo "Downloading XCFramework from GitHub releases..."

if [ "$BUILD_TYPE" = "debug" ]; then
    echo "❌ Error: Debug builds are not provided in releases."
    echo "   For local development:"
    echo "   1. Build locally: ./scripts/build_xcframework.sh --debug"
    echo "   2. Or set NOBODYWHO_SWIFT_XCFRAMEWORK_PATH"
    exit 1
fi

# Determine download URL
if [ "$VERSION" = "latest" ]; then
    # Get latest release tag
    echo "Fetching latest release version..."
    RELEASE_URL="https://api.github.com/repos/nobodywho-ooo/nobodywho/releases/latest"
    VERSION=$(curl -s "$RELEASE_URL" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' | sed 's/^v//')

    if [ -z "$VERSION" ] || [ "$VERSION" = "null" ]; then
        echo "❌ Error: Could not determine latest release version"
        echo "   No releases found. You must build locally:"
        echo "   ./scripts/build_xcframework.sh"
        exit 1
    fi

    echo "Latest version: $VERSION"
fi

DOWNLOAD_URL="https://github.com/nobodywho-ooo/nobodywho/releases/download/v${VERSION}/NobodyWhoFFI.xcframework.zip"
ZIP_PATH="$CACHE_DIR/$VERSION/NobodyWhoFFI.xcframework.zip"

# Create cache directory
mkdir -p "$(dirname "$ZIP_PATH")"

echo "Downloading: $DOWNLOAD_URL"

# Download with progress
if command -v curl >/dev/null 2>&1; then
    HTTP_CODE=$(curl -L -w "%{http_code}" -o "$ZIP_PATH" "$DOWNLOAD_URL" 2>/dev/null)

    if [ "$HTTP_CODE" != "200" ]; then
        rm -f "$ZIP_PATH"
        echo "❌ Error: Failed to download (HTTP $HTTP_CODE)"
        echo "   URL: $DOWNLOAD_URL"
        echo "   This version may not have Swift SDK artifacts published yet."
        echo "   Build locally instead: ./scripts/build_xcframework.sh"
        exit 1
    fi
else
    echo "❌ Error: curl not found"
    exit 1
fi

echo "✓ Downloaded to: $ZIP_PATH"

# Extract
echo "Extracting XCFramework..."
unzip -q -o "$ZIP_PATH" -d "$(dirname "$CACHE_PATH")"

if [ ! -d "$CACHE_PATH" ]; then
    echo "❌ Error: XCFramework not found after extraction"
    exit 1
fi

# Clean up zip
rm -f "$ZIP_PATH"

# Copy to final location
echo "Installing XCFramework..."
cp -R "$CACHE_PATH" "$XCFRAMEWORK_OUTPUT"

echo "✓ XCFramework ready at: $XCFRAMEWORK_OUTPUT"
echo ""
echo "Setup complete!"
