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
            path: "nobodywho/swift/Sources/NobodyWho",
            linkerSettings: [
                .linkedFramework("NobodyWhoFFI")
            ]
        ),
        // XCFramework distributed via GitHub releases
        .binaryTarget(
            name: "NobodyWhoFFI",
            url: "https://github.com/Intiserahmed/nobodywho/releases/download/nobodywho-swift-v0.1.0/NobodyWhoFFI.xcframework.zip",
            checksum: "51d0103b8d63c9360bfb2d2d08017bd19eac45fc12de7fa29e68582f4366efb3"
        )
    ]
)
