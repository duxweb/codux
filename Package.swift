// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "DmuxWorkspace",
    platforms: [
        .macOS(.v14),
    ],
    products: [
        .executable(
            name: "dmux",
            targets: ["DmuxWorkspace"]
        ),
        .executable(
            name: "dmux-notify-helper",
            targets: ["DmuxNotifyHelper"]
        ),
    ],
    dependencies: [
        .package(url: "https://github.com/migueldeicaza/SwiftTerm.git", branch: "main"),
    ],
    targets: [
        .executableTarget(
            name: "DmuxWorkspace",
            dependencies: [
                .product(name: "SwiftTerm", package: "SwiftTerm"),
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
        .executableTarget(
            name: "DmuxNotifyHelper",
            path: "Sources/DmuxNotifyHelper",
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("Foundation"),
                .linkedFramework("UserNotifications"),
            ]
        ),
    ]
)
