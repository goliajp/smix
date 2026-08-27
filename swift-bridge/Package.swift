// swift-tools-version: 5.9
import PackageDescription

let package = Package(
  name: "SmixRunnerCore",
  platforms: [.iOS(.v17), .macOS(.v13)],
  products: [
    .library(name: "SmixRunnerCore", targets: ["SmixRunnerCore"]),
    .library(name: "SmixIndigoHID", targets: ["SmixIndigoHID"]),
    .library(name: "SmixCoreFFIBindings", targets: ["SmixCoreFFIBindings"]),
    .library(name: "SmixSDK", targets: ["SmixSDK"]),
    .executable(name: "smix-host-hid", targets: ["SmixHostHID"]),
    .executable(name: "smix-capture-host", targets: ["SmixCaptureHost"]),
    .executable(name: "SwiftFixtureRunner", targets: ["SwiftFixtureRunner"]),
  ],
  dependencies: [
    .package(url: "https://github.com/swhitty/FlyingFox.git", .upToNextMajor(from: "0.26.0")),
  ],
  targets: [
    .target(
      name: "SmixRunnerCore",
      dependencies: [.product(name: "FlyingFox", package: "FlyingFox")]
    ),
    .testTarget(
      name: "SmixRunnerCoreTests",
      dependencies: ["SmixRunnerCore", .product(name: "FlyingFox", package: "FlyingFox")]
    ),
    .target(name: "SmixIndigoHID", dependencies: ["SmixRunnerCore"]),
    .testTarget(name: "SmixIndigoHIDTests", dependencies: ["SmixIndigoHID"]),
    .executableTarget(name: "SmixHostHID", dependencies: ["SmixIndigoHID"]),
    .executableTarget(name: "SmixCaptureHost"),

    // SmixCoreFFI binary target (Rust core via UniFFI 0.29.5)
    // built by `scripts/sdk/build-xcframework.sh` from `crates/smix-ffi`.
    // Slices: ios-arm64-simulator + macos-arm64 (simulator, plus the
    // macOS host test). The real-device slice (ios-arm64) is deliberately
    // NOT included: smix supports the simulator only.
    .binaryTarget(name: "SmixCoreFFI", path: "SmixCoreFFI.xcframework"),

    // Swift wrapper around SmixCoreFFI (UniFFI-generated
    // bindings + thin idiomatic API). Generated files live under
    // Sources/SmixCoreFFIBindings/Generated (smix.swift,
    // smixFFI.h re-exported via module.modulemap inside .xcframework).
    .target(
      name: "SmixCoreFFIBindings",
      dependencies: ["SmixCoreFFI"],
      path: "Sources/SmixCoreFFIBindings"
    ),
    .testTarget(
      name: "SmixCoreFFITests",
      dependencies: ["SmixCoreFFIBindings"],
      path: "Tests/SmixCoreFFITests"
    ),

    // SmixSDK ergonomic facade target.
    // Playwright-style Swift API: Smix.launchApp + App actor +
    // Selector enum + Locator + ExpectationFailure. Mirrors the Rust
    // smix-sdk surface.
    .target(
      name: "SmixSDK",
      dependencies: ["SmixCoreFFIBindings", "SmixRunnerCore"],
      path: "Sources/SmixSDK"
    ),
    .testTarget(
      name: "SmixSDKTests",
      dependencies: ["SmixSDK"],
      path: "Tests/SmixSDKTests",
      // The same payloads the Rust reader runs against. Three readers of
      // one document only stay honest if they are pointed at one set of
      // recorded bytes.
      resources: [.copy("fixtures")]
    ),

    // Cross-binary conformance fixture runner. Loads
    // a fixture JSON, calls SmixCoreFFIBindings.resolveSelector, prints
    // sorted JSON id array to stdout. Paired with the Rust
    // fixture-runner bin for byte-identical diff via
    // scripts/sdk/run-cross-binary-harness.sh.
    .executableTarget(
      name: "SwiftFixtureRunner",
      dependencies: ["SmixCoreFFIBindings"],
      path: "Sources/SwiftFixtureRunner"
    ),
  ]
)
