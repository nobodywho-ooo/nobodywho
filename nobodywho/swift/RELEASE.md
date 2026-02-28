# Release Process for Swift SDK

## Overview

The Swift SDK distributes pre-built XCFramework binaries via GitHub releases. SPM automatically downloads the correct binary for users.

## For Users

Simply add the package - SPM handles everything:

```swift
dependencies: [
    .package(url: "https://github.com/nobodywho-ooo/nobodywho", from: "0.1.0")
]
```

No build steps required.

## For Maintainers

### Creating a New Release

1. **Build XCFramework**
   ```bash
   cd nobodywho/swift
   ./scripts/build_xcframework.sh
   ```

2. **Create Zip**
   ```bash
   zip -r -y NobodyWhoFFI.xcframework.zip NobodyWhoFFI.xcframework
   ```

3. **Calculate Checksum**
   ```bash
   swift package compute-checksum NobodyWhoFFI.xcframework.zip
   ```

4. **Update Package.swift**

   Update the binaryTarget with new version and checksum:
   ```swift
   .binaryTarget(
       name: "NobodyWhoFFI",
       url: "https://github.com/nobodywho-ooo/nobodywho/releases/download/nobodywho-swift-v0.2.0/NobodyWhoFFI.xcframework.zip",
       checksum: "NEW_CHECKSUM_HERE"
   )
   ```

5. **Create Git Tag**
   ```bash
   git add nobodywho/swift/Package.swift
   git commit -m "chore: Swift SDK v0.2.0"
   git tag -a nobodywho-swift-v0.2.0 -m "Swift SDK v0.2.0"
   git push origin main
   git push origin nobodywho-swift-v0.2.0
   ```

6. **Create GitHub Release**
   - Go to: https://github.com/nobodywho-ooo/nobodywho/releases/new
   - Tag: `nobodywho-swift-v0.2.0`
   - Upload: `NobodyWhoFFI.xcframework.zip`
   - Publish release

7. **Verify**

   Test that SPM can download:
   ```bash
   swift package resolve
   swift build
   ```

## CI Integration

The GitHub Actions workflow `.github/workflows/build.yml` includes a `build-swift-xcframework` job that:
- Builds the XCFramework for all Apple platforms
- Creates the zip file
- Uploads as an artifact

Maintainers can download this artifact and attach it to releases.

## Binary Distribution

- **XCFramework is NOT committed** to git
- Users download via SPM from GitHub releases
- Contributors build locally with `./scripts/build_xcframework.sh`
- CI builds and provides artifacts for releases
