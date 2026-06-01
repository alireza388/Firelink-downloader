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
    targets: [
        .executableTarget(
            name: "Firelink",
            path: "Sources/Firelink"
        )
    ]
)
