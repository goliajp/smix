// SmixSDK Kotlin top-level entry.
//
// Mirrors swift-bridge/Sources/SmixSDK/Smix.swift. Playwright-style
// ergonomic facade backed by UniFFI Kotlin bindings (uniffi.smix.*).
//
// Usage:
//   val runtime = MockSimRuntime()  // or an HTTP-backed real runtime
//   val app = Smix.launchApp(AppTarget.BundleId("com.example.MyApp"), runtime)
//   app.tap(Selector.Id("btn-login"))
//   app.find(Selector.Text("Welcome")).toBeVisible(timeout = 5.seconds)
//
// This module is intended for test targets only
// (androidTestImplementation / debugImplementation); there is no linkage
// path into an app's release build.

package dev.smix.sdk

/**
 * Top-level entry to the smix SDK. Launches and attaches to Android
 * emulator apps via the supplied [SmixSimRuntime].
 */
object Smix {
    /**
     * Launch the target app on the emulator via [runtime] and return
     * an [App] handle.
     */
    suspend fun launchApp(
        target: AppTarget,
        runtime: SmixSimRuntime,
        resolver: SelectorResolver = DefaultFfiResolver,
        labelsResolver: LabelResolver = DefaultFfiLabelsResolver,
    ): App = when (target) {
        is AppTarget.BundleId -> {
            runtime.launch(target.value)
            App(bundleId = target.value, runtime = runtime,
                resolver = resolver, labelsResolver = labelsResolver)
        }
        is AppTarget.AppPath -> {
            runtime.launchFromPath(target.path)
            App(bundleId = target.path, runtime = runtime,
                resolver = resolver, labelsResolver = labelsResolver)
        }
    }
}

/**
 * What to launch — either a registered bundle identifier or an absolute
 * path to an installed APK / app dir.
 */
sealed interface AppTarget {
    data class BundleId(val value: String) : AppTarget
    data class AppPath(val path: String) : AppTarget
}

/**
 * SDK error category for compile-time-known unimplemented surface,
 * distinct from [ExpectationFailure] (runtime resolution failure).
 */
sealed class SmixError(message: String) : Exception(message) {
    class NotImplemented(val stage: String, val api: String) :
        SmixError("$api not implemented yet ($stage)")
}
