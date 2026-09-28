// The limit each polling route keeps to, in milliseconds.
//
// The same number as the route's `// LONGEST WAIT` statement in RunnerTest
// and as the host's table (crates/smix-runner-client/src/route_limits.rs:
// the route's own waits plus one look past each poll), which is the one the
// host waits by; runner-waits-fit-the-host holds the three equal. A route
// that keeps to it answers before the host stops waiting.

package dev.smix.runner

object RouteLimits {
    val MS: Map<String, Long> = mapOf(
        "/back" to 6_000L,
        "/hide-keyboard" to 6_500L,
        "/set-orientation" to 7_800L,
        "/tap-by-id" to 8_075L,
        "/clear-text" to 19_000L,
        "/foreground" to 7_500L,
    )

    fun of(path: String): Long =
        MS[path] ?: throw IllegalArgumentException("no route limit for $path")
}
