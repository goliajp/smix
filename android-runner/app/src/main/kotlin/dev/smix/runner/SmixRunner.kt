// v6.0 c3b — placeholder main-source class.
//
// Empty namespace anchor. Actual instrumentation runner + HTTP server
// live under androidTest/ (see RunnerTest.kt).

package dev.smix.runner

internal object SmixRunner {
    /// Build identifier surfaced via GET /health route.
    const val VERSION: String = "v6.0-c3b"
}
