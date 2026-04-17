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
        .package(url: "https://github.com/migueldeicaza/SwiftTerm.git", branch: "main"),
        .package(url: "https://github.com/sparkle-project/Sparkle", from: "2.9.1"),
    ],
    targets: [
        .executableTarget(
            name: "DmuxWorkspace",
            dependencies: [
                .product(name: "SwiftTerm", package: "SwiftTerm"),
                .product(name: "Sparkle", package: "Sparkle"),
            ],
            path: "Sources/DmuxWorkspace",
            resources: [
                .process("Resources"),
            ],
            linkerSettings: [
                .linkedFramework("UniformTypeIdentifiers"),
                .linkedFramework("Carbon"),
                .linkedLibrary("sqlite3"),
            ]
        ),
    ]
)
