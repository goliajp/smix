// Placeholder main-source class (empty namespace anchor). Actual
// instrumentation runner + HTTP server live under androidTest/ (see
// RunnerTest.kt).
//
// VERSION tracks the smix workspace version and is bumped as part of
// every release; `scripts/release/ship.sh` gates on it matching the
// ship version, so this string cannot drift behind the workspace.

package dev.smix.runner

internal object SmixRunner {
    /// Build identifier surfaced via GET /health route.
    const val VERSION: String = "9.0.0"
}
