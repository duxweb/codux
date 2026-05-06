// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "LlamaBinary",
    platforms: [
        .macOS(.v14),
    ],
    products: [
        .library(name: "llama", targets: ["llama"]),
    ],
    targets: [
        .binaryTarget(
            name: "llama",
            url: "https://github.com/ggml-org/llama.cpp/releases/download/b9041/llama-b9041-xcframework.zip",
            checksum: "28f7e7d7a2d7c3c4fccad32501fd6de958a9be40121c103001e8b048f5f85121"
        ),
    ]
)
