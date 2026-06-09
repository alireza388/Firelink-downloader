// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "Firelink",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "Firelink", targets: ["Firelink"])
    ],
    dependencies: [],
    targets: [
        .executableTarget(
            name: "Firelink",
            dependencies: [],
            path: "Sources/Firelink",
            exclude: [
                "_internal"
            ],
            resources: [
                .process("Assets.xcassets"),
                .copy("yt-dlp"),
                .copy("ffmpeg")
            ]
        )
    ]
)
