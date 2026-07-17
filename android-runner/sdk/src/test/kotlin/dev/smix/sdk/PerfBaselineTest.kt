// Kotlin perf baseline. Measures App.tap round-trip against the
// in-memory MockDriver / MockSession / MockSelectorResolver seams.
// Output to stdout (capture via `./gradlew :sdk:testDebugUnitTest
// --tests PerfBaselineTest -i 2>&1 | grep '^perf:'`).
//
// Gated by SMIX_PERF_BENCH=1 env var; skipped in regression suite.

package dev.smix.sdk

import kotlinx.coroutines.runBlocking
import org.junit.Assume
import org.junit.Test

class PerfBaselineTest {

    @Test
    fun testTapRoundTripLatency() = runBlocking {
        Assume.assumeTrue(
            "set SMIX_PERF_BENCH=1 to run perf baseline",
            System.getenv("SMIX_PERF_BENCH") == "1",
        )

        val iterations = 1000
        val warmup = 100

        val button = A11yNode(
            rawType = "button",
            identifier = "btn-x",
            label = "Tap me",
            bounds = Rect(100.0, 200.0, 80.0, 40.0),
            enabled = true,
            visible = true,
        )
        val tree = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 393.0, 852.0),
            enabled = true,
            visible = true,
            children = listOf(button),
        )

        fun setupApp(): App {
            val resolver = MockSelectorResolver().apply {
                registerHit("""{"id":"btn-x"}""", "btn-x")
            }
            return mockApp(tree = tree, resolver = resolver)
        }

        // Warmup
        repeat(warmup) {
            val app = setupApp()
            app.tap(Selector.Id("btn-x"))
        }

        // Measure
        val samples = mutableListOf<Double>()
        repeat(iterations) {
            val app = setupApp()
            val start = System.nanoTime()
            app.tap(Selector.Id("btn-x"))
            samples.add((System.nanoTime() - start) / 1_000_000.0)  // ms
        }
        samples.sort()

        val median = samples[samples.size / 2]
        val p99 = samples[(samples.size * 0.99).toInt()]
        val minV = samples.first()
        val maxV = samples.last()
        val avg = samples.average()

        println("perf: # SmixSDK Kotlin perf baseline")
        println("perf: # Date: ${java.time.Instant.now()}")
        println("perf: # Operation: Smix.launchApp + App.tap(Selector.Id)")
        println("perf: # Backend: MockDriver + MockSession + MockSelectorResolver (in-memory)")
        println("perf: # Iterations: $iterations (after $warmup warmup)")
        println("perf: ")
        println("perf: min:    ${"%.3f".format(minV)} ms")
        println("perf: avg:    ${"%.3f".format(avg)} ms")
        println("perf: median: ${"%.3f".format(median)} ms")
        println("perf: p99:    ${"%.3f".format(p99)} ms")
        println("perf: max:    ${"%.3f".format(maxV)} ms")
        println("perf: ")
        println("perf: # Regression gate:")
        println("perf: #   soft fail if median > ${"%.3f".format(median * 1.5)} ms (1.5x)")
        println("perf: #   hard fail if median > ${"%.3f".format(median * 3)} ms (3x)")
    }
}
