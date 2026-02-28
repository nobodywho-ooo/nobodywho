// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "NobodyWho",
    platforms: [
        .iOS(.v13),
        .macOS(.v11)
    ],
    products: [
        .library(
            name: "NobodyWho",
            targets: ["NobodyWho"]
        ),
        .executable(
            name: "NobodyWhoTestCLI",
            targets: ["NobodyWhoTestCLI"]
        ),
    ],
    dependencies: [],
    targets: [
        .target(
            name: "NobodyWho",
            dependencies: ["NobodyWhoFFI"],
            path: "Sources/NobodyWho",
            cSettings: [
                .unsafeFlags(["-fmodule-map-file=NobodyWhoFFI.xcframework/macos-arm64_x86_64/NobodyWhoFFI.framework/Modules/module.modulemap"], .when(platforms: [.macOS])),
                .unsafeFlags(["-fmodule-map-file=NobodyWhoFFI.xcframework/ios-arm64/NobodyWhoFFI.framework/Modules/module.modulemap"], .when(platforms: [.iOS]))
            ],
            linkerSettings: [
                .linkedFramework("NobodyWhoFFI")
            ]
        ),
        // XCFramework distributed via GitHub releases
        .binaryTarget(
            name: "NobodyWhoFFI",
            url: "https://github.com/nobodywho-ooo/nobodywho/releases/download/nobodywho-swift-v0.1.0/NobodyWhoFFI.xcframework.zip",
            checksum: "120b7e51aef498ae958d32a7adb79ab02839e8e8bf3963a0382af1b2b7138626"
        ),
        .executableTarget(
            name: "NobodyWhoTestCLI",
            dependencies: ["NobodyWho"],
            path: "Sources/NobodyWhoTestCLI",
            cSettings: [
                .unsafeFlags(["-fmodule-map-file=NobodyWhoFFI.xcframework/macos-arm64_x86_64/NobodyWhoFFI.framework/Modules/module.modulemap"], .when(platforms: [.macOS])),
                .unsafeFlags(["-fmodule-map-file=NobodyWhoFFI.xcframework/ios-arm64/NobodyWhoFFI.framework/Modules/module.modulemap"], .when(platforms: [.iOS]))
            ]
        ),
        .testTarget(
            name: "NobodyWhoTests",
            dependencies: ["NobodyWho"],
            cSettings: [
                .unsafeFlags(["-fmodule-map-file=NobodyWhoFFI.xcframework/macos-arm64_x86_64/NobodyWhoFFI.framework/Modules/module.modulemap"], .when(platforms: [.macOS])),
                .unsafeFlags(["-fmodule-map-file=NobodyWhoFFI.xcframework/ios-arm64/NobodyWhoFFI.framework/Modules/module.modulemap"], .when(platforms: [.iOS]))
            ]
        ),
    ]
)
