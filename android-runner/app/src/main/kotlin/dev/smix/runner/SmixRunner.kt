// Placeholder main-source class (empty namespace anchor). Actual
// instrumentation runner + HTTP server live under androidTest/ (see
// RunnerTest.kt).
//
// VERSION tracks the smix workspace version and is bumped as part of
// every release (`scripts/release/ship.sh` gates on it matching the
// ship version — v1.0.26 closed the drift where this string froze at
// an old build id while the workspace advanced).

package dev.smix.runner

internal object SmixRunner {
    /// Build identifier surfaced via GET /health route.
    const val VERSION: String = "1.0.27"
}
