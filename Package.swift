// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "DmuxWorkspace",
    defaultLocalization: "en",
    platforms: [
        .macOS(.v14),
    ],
    products: [
        .executable(
            name: "dmux",
            targets: ["DmuxWorkspace"]
        ),
    ],
    dependencies: [
        .package(url: "https://github.com/duxphp/libghostty-spm.git", branch: "pr/appkit-input-alignment"),
        .package(url: "https://github.com/sparkle-project/Sparkle", from: "2.9.1"),
        .package(url: "https://github.com/apple/swift-log.git", from: "1.6.0"),
    ],
    targets: [
        .executableTarget(
            name: "DmuxWorkspace",
            dependencies: [
                .product(name: "GhosttyTerminal", package: "libghostty-spm"),
                .product(name: "GhosttyTheme", package: "libghostty-spm"),
                .product(name: "Sparkle", package: "Sparkle"),
                .product(name: "Logging", package: "swift-log"),
            ],
            path: "Sources/DmuxWorkspace",
            resources: [
                .copy("Resources"),
            ],
            linkerSettings: [
                .linkedFramework("UniformTypeIdentifiers"),
                .linkedFramework("Carbon"),
                .linkedLibrary("sqlite3"),
            ]
        ),
        .testTarget(
            name: "DmuxWorkspaceTests",
            dependencies: ["DmuxWorkspace"],
            path: "Tests/DmuxWorkspaceTests"
        ),
    ]
)
