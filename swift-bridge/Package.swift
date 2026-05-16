// swift-tools-version: 5.9
import PackageDescription

let package = Package(
  name: "SimxRunnerCore",
  platforms: [.iOS(.v17), .macOS(.v13)],
  products: [
    .library(name: "SimxRunnerCore", targets: ["SimxRunnerCore"]),
    .library(name: "SimxIndigoHID", targets: ["SimxIndigoHID"]),
    .executable(name: "simx-host-hid", targets: ["SimxHostHID"]),
  ],
  dependencies: [
    .package(url: "https://github.com/swhitty/FlyingFox.git", .upToNextMajor(from: "0.26.0")),
  ],
  targets: [
    .target(
      name: "SimxRunnerCore",
      dependencies: [.product(name: "FlyingFox", package: "FlyingFox")]
    ),
    .testTarget(
      name: "SimxRunnerCoreTests",
      dependencies: ["SimxRunnerCore", .product(name: "FlyingFox", package: "FlyingFox")]
    ),
    .target(name: "SimxIndigoHID"),
    .testTarget(name: "SimxIndigoHIDTests", dependencies: ["SimxIndigoHID"]),
    .executableTarget(name: "SimxHostHID", dependencies: ["SimxIndigoHID"]),
  ]
)
