// swift-tools-version: 6.2

import PackageDescription

// The Rust core (core/, ADR-0007) as the `LocalhostCore` module the app
// imports. Run scripts/build-core.sh first: it builds the xcframework and the
// generated Swift this package compiles.
let package = Package(
    name: "LocalhostCoreFFI",
    platforms: [.macOS(.v15)],
    products: [
        .library(name: "LocalhostCore", targets: ["LocalhostCore"]),
    ],
    targets: [
        .binaryTarget(name: "LocalhostCoreFFI", path: "Generated/LocalhostCoreFFI.xcframework"),
        .target(
            name: "LocalhostCore",
            dependencies: ["LocalhostCoreFFI"],
            linkerSettings: [.linkedFramework("Security"), .linkedFramework("CoreFoundation")]
        ),
        .testTarget(name: "LocalhostCoreTests", dependencies: ["LocalhostCore"]),
    ],
    swiftLanguageModes: [.v6]
)
