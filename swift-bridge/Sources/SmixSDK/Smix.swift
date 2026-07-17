// Smix top-level entry — wires `Smix.launchApp` over the FFI driving
// surface. Constructs an `SmixDriver` pointed at the local runner, opens
// a session for the target bundle id, launches it, and hands back an
// `App` holding both handles.

import Foundation
import SmixCoreFFIBindings
import SmixRunnerCore

/// Top-level entry to the smix SDK. Launches and attaches to iOS
/// Simulator apps over the FFI boundary to a running smix runner.
public enum Smix {
    /// Default local runner port (mirror `RunnerPortResolver.defaultPort`).
    public static let defaultRunnerPort: UInt16 = RunnerPortResolver.defaultPort

    /// Launch the target app on the simulator and return an [`App`]
    /// handle.
    ///
    /// Constructs an [`SmixDriver`] on `port`, opens a session bound to
    /// the target bundle id, launches the app, and returns an [`App`]
    /// holding the driver (tree / list) and session (act).
    ///
    /// - Parameters:
    ///   - target: bundle identifier to launch (must already be
    ///     installed on the sim).
    ///   - port: local runner port. Defaults to
    ///     [`Smix.defaultRunnerPort`].
    /// - Returns: an [`App`] handle bound to the launched process.
    public static func launchApp(
        _ target: AppTarget,
        port: UInt16 = Smix.defaultRunnerPort
    ) async throws -> App {
        switch target {
        case let .bundleId(id):
            let driver = SmixDriver(port: port)
            let session = try await driver.openSession(bundleId: id, cancel: nil)
            try await session.launchApp(cancel: nil)
            return App(bundleId: id, driver: driver, session: session)
        }
    }
}

/// What to launch — a registered bundle identifier already installed on
/// the sim.
public enum AppTarget: Sendable, Equatable {
    /// e.g. `com.example.MyApp` — must already be installed on sim.
    case bundleId(String)
}

/// SDK error category used for compile-time-known unimplemented
/// surface, distinct from [`ExpectationFailure`] (runtime resolution
/// failure).
public enum SmixError: Error, Sendable, Equatable {
    /// Method exists on the API surface but body is not yet wired.
    case notImplemented(stage: String, api: String)
}
