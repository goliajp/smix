// Smix top-level entry wires Smix.launchApp via SimRuntime.
//
// The SDK is XCTest-free; test author supplies a concrete SmixSimRuntime
// impl (an XCUITest-backed runtime for real-sim runs; MockSimRuntime here
// for unit tests).

import Foundation

/// Top-level entry to the smix SDK. Launches and attaches to iOS Simulator
/// apps via the supplied [`SmixSimRuntime`].
public enum Smix {
    /// Launch the target app on the simulator via `runtime` and return
    /// an [`App`] handle.
    ///
    /// - Parameters:
    ///   - target: bundle identifier or absolute `.app` path.
    ///   - runtime: low-level simulator I/O runtime (real impl is an
    ///     XCUITest-backed runtime in the consumer's XCUITest target;
    ///     `MockSimRuntime` for unit tests).
    /// - Returns: an [`App`] handle bound to the launched process.
    /// - Throws: [`SmixError.notImplemented`] for surface not yet wired.
    public static func launchApp(
        _ target: AppTarget,
        runtime: SmixSimRuntime
    ) async throws -> App {
        switch target {
        case let .bundleId(id):
            try await runtime.launch(bundleId: id)
            return App(bundleId: id, runtime: runtime)
        case let .appPath(path):
            try await runtime.launchFromPath(path)
            // Note: bundleId is unknown until the app is installed and
            // its Info.plist is parsed. The runtime impl is expected to
            // resolve and return the bundleId via a follow-up call; for
            // MVP we store the appPath as the SDK-side handle. Tests
            // can inspect runtime.launchFromPathCalls to verify wire.
            return App(bundleId: path, runtime: runtime)
        }
    }
}

/// What to launch — either a registered bundle identifier or an absolute
/// path to a built `.app` bundle.
public enum AppTarget: Sendable, Equatable {
    /// e.g. `com.example.MyApp` — must already be installed on sim.
    case bundleId(String)
    /// Absolute path to `MyApp.app` — used by smix CLI `--app-path`.
    case appPath(String)
}

/// SDK error category used for compile-time-known unimplemented
/// surface, distinct from [`ExpectationFailure`] (runtime resolution
/// failure).
public enum SmixError: Error, Sendable, Equatable {
    /// Method exists on the API surface but body is not yet wired.
    case notImplemented(stage: String, api: String)
}
