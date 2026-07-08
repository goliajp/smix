// swift-tools-version: 5.9
//
// Root-level Package.swift so SwiftPM consumers can
// `.package(url: "https://github.com/goliajp/smix", from: "0.1.0")` directly.
//
// Mirrors swift-bridge/Package.swift with `swift-bridge/` prefix on all paths.
// The original swift-bridge/Package.swift stays for backward compat with
// existing scripts that use `swift --package-path swift-bridge`.

import PackageDescription

let package = Package(
  name: "smix",
  platforms: [.iOS(.v17), .macOS(.v13)],
  products: [
    // Public products for external consumers (SDK + FFI bindings).
    .library(name: "SmixSDK", targets: ["SmixSDK"]),
    .library(name: "SmixCoreFFIBindings", targets: ["SmixCoreFFIBindings"]),

    // Internal libraries (still public-exportable but mainly for
    // SmixRunnerUITests + external SDK consumers).
    .library(name: "SmixRunnerCore", targets: ["SmixRunnerCore"]),
    .library(name: "SmixIndigoHID", targets: ["SmixIndigoHID"]),
  ],
  dependencies: [
    .package(url: "https://github.com/swhitty/FlyingFox.git", .upToNextMajor(from: "0.26.0")),
  ],
  targets: [
    .target(
      name: "SmixRunnerCore",
      dependencies: [.product(name: "FlyingFox", package: "FlyingFox")],
      path: "swift-bridge/Sources/SmixRunnerCore"
    ),
    .target(
      name: "SmixIndigoHID",
      dependencies: ["SmixRunnerCore"],
      path: "swift-bridge/Sources/SmixIndigoHID"
    ),

    // SmixCoreFFI binary target (Rust core via UniFFI 0.29.5)
    // built by `scripts/sdk/build-xcframework.sh` from `crates/smix-ffi`.
    .binaryTarget(name: "SmixCoreFFI", path: "swift-bridge/SmixCoreFFI.xcframework"),

    // Swift wrapper around SmixCoreFFI (UniFFI-generated bindings).
    .target(
      name: "SmixCoreFFIBindings",
      dependencies: ["SmixCoreFFI"],
      path: "swift-bridge/Sources/SmixCoreFFIBindings"
    ),

    // SmixSDK ergonomic facade — main public consumer surface.
    .target(
      name: "SmixSDK",
      dependencies: ["SmixCoreFFIBindings", "SmixRunnerCore"],
      path: "swift-bridge/Sources/SmixSDK"
    ),
  ]
)
