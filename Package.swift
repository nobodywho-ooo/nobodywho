// swift-tools-version: 5.9
import PackageDescription

// Root package manifest for the NobodyWho monorepo
// This allows SPM to find the Swift SDK in the nobodywho/swift subdirectory

let package = Package(
    name: "nobodywho",
    platforms: [
        .iOS(.v13),
        .macOS(.v11)
    ],
    products: [
        .library(
            name: "NobodyWho",
            targets: ["NobodyWho"]
        )
    ],
    dependencies: [],
    targets: [
        .target(
            name: "NobodyWho",
            dependencies: ["NobodyWhoFFI"],
            path: "nobodywho/swift/Sources/NobodyWho"
        ),
        // XCFramework distributed via GitHub releases
        .binaryTarget(
            name: "NobodyWhoFFI",
            url: "https://github.com/Intiserahmed/nobodywho/releases/download/nobodywho-swift-v0.1.0/NobodyWhoFFI.xcframework.zip",
            checksum: "120b7e51aef498ae958d32a7adb79ab02839e8e8bf3963a0382af1b2b7138626"
        )
    ]
)
