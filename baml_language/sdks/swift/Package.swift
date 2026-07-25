// swift-tools-version: 6.1
// BAML Swift SDK runtime package.
//
// `BamlBridgeFFI` is the Rust bridge staticlib (bridge_cffi via
// sdks/swift/rust/bridge_swift) wrapped in an XCFramework together with
// the C header + modulemap from Sources/CBamlBridge/include. It is NOT
// checked in — build it first:
//
//   sdks/swift/scripts/build-xcframework.sh --host-only
//
// (sdk_tests/crates/swift/setup.sh does this automatically for test runs;
// released packages substitute a remote binaryTarget with a checksum.)
import PackageDescription

let package = Package(
    name: "baml-swift",
    platforms: [
        .macOS(.v13),
        .iOS(.v16),
    ],
    products: [
        .library(name: "BamlBridge", targets: ["BamlBridge"])
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-protobuf.git", from: "1.38.0")
    ],
    targets: [
        .binaryTarget(
            name: "BamlBridgeFFI",
            path: "Binaries/BamlBridgeFFI.xcframework"
        ),
        .target(
            name: "BamlBridge",
            dependencies: [
                "BamlBridgeFFI",
                .product(name: "SwiftProtobuf", package: "swift-protobuf"),
            ],
            linkerSettings: [
                // System deps of the Rust staticlib (TLS roots, DNS, CF).
                .linkedFramework("CoreFoundation"),
                .linkedFramework("Security"),
                .linkedFramework("SystemConfiguration"),
                .linkedLibrary("resolv"),
            ]
        ),
        .testTarget(
            name: "BamlBridgeTests",
            dependencies: ["BamlBridge"]
        ),
    ]
)
