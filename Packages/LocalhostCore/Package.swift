// swift-tools-version: 6.2

import PackageDescription

let package = Package(
    name: "LocalhostCore",
    platforms: [.macOS(.v15)],
    products: [
        .library(name: "LocalhostCore", targets: ["LocalhostCore"]),
    ],
    targets: [
        .target(name: "LocalhostCore"),
        .testTarget(name: "LocalhostCoreTests", dependencies: ["LocalhostCore"]),
    ],
    swiftLanguageModes: [.v6]
)
