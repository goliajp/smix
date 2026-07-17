// SmixSDK Kotlin top-level entry.
//
// Mirrors swift-bridge/Sources/SmixSDK/Smix.swift. Playwright-style
// ergonomic facade backed by UniFFI Kotlin bindings (uniffi.smix.*).
//
// Usage:
//   val app = Smix.launchApp(AppTarget.BundleId("com.example.MyApp"))
//   app.tap(Selector.Id("btn-login"))
//   app.find(Selector.Text("Welcome")).toBeVisible(timeout = 5.seconds)
//
// This module is intended for test targets only
// (androidTestImplementation / debugImplementation); there is no linkage
// path into an app's release build.

package dev.smix.sdk

/**
 * Top-level entry to the smix SDK. Launches and attaches to Android
 * emulator apps over the FFI boundary to a running smix runner.
 */
object Smix {
    /**
     * Default local runner port. The Android runner listens on 28080 by
     * convention (see docs/ai-guide/05-cli.md; iOS uses 22087).
     */
    val defaultRunnerPort: UShort = 28080u

    /**
     * Launch the target app on the emulator and return an [App] handle.
     *
     * Constructs a [Driver] on [port], opens a session bound to the
     * target bundle id, launches the app, and returns an [App] holding
     * the driver (tree / list) and session (act).
     */
    suspend fun launchApp(
        target: AppTarget,
        port: UShort = defaultRunnerPort,
        resolver: SelectorResolver = DefaultFfiResolver,
        labelsResolver: LabelResolver = DefaultFfiLabelsResolver,
    ): App = when (target) {
        is AppTarget.BundleId -> {
            val driver = FfiDriver(port)
            val session = driver.openSession(target.value)
            session.launchApp()
            App(
                bundleId = target.value,
                driver = driver,
                session = session,
                resolver = resolver,
                labelsResolver = labelsResolver,
            )
        }
    }
}

/**
 * What to launch — a registered bundle identifier already installed on
 * the emulator.
 */
sealed interface AppTarget {
    data class BundleId(val value: String) : AppTarget
}

/**
 * SDK error category for compile-time-known unimplemented surface,
 * distinct from [ExpectationFailure] (runtime resolution failure).
 */
sealed class SmixError(message: String) : Exception(message) {
    class NotImplemented(val stage: String, val api: String) :
        SmixError("$api not implemented yet ($stage)")
}
