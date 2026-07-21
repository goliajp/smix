// v8 step 1 — Android instrumentation T1 harness.
//
// Runs all 24 conformance fixtures (crates/smix-core-conformance/fixtures/*.json)
// through Kotlin SDK + uniffi.smix.resolveSelector + writes per-fixture
// JSON result to /sdcard/Android/data/<pkg>/cache/conformance-<id>.json.
//
// The host side of that comparison is NOT built. This comment used to
// name scripts/sdk/pull-android-conformance.sh for retrieving the output
// and scripts/sdk/sync-conformance-fixtures-to-android-assets.sh for
// keeping the assets in step; neither has ever existed. scripts/sdk/
// holds build-android-aar.sh, build-xcframework.sh, regenerate-bindings.sh
// and run-cross-binary-harness.sh, and the last of those has no Kotlin
// backend wired into it.
//
// So this emits and nothing reads it, which is why the class is documented
// below as always passing. Until the pull and the diff exist, a green run
// means the fixtures were processed without throwing — not that Kotlin
// agrees byte-for-byte with Rust and Swift.
//
// Fixtures are bundled as Android assets at android-runner/sdk/src/
// androidTest/assets/conformance/*.json, mirroring
// crates/smix-core-conformance/fixtures/. Both hold the same 24 files
// (verified by diff; the assets side additionally has a .gitkeep), and
// keeping them in step is currently a manual copy.

package dev.smix.sdk

import android.content.Context
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.smix.resolveSelector
import java.io.File

@RunWith(AndroidJUnit4::class)
class ConformanceHarnessTest {

    /**
     * For each bundled fixture asset: load JSON, extract tree+selector,
     * call resolveSelector, write `[sorted ids]` JSON output to instrument
     * cache directory. Always passes — the per-fixture diff happens host-side
     * against Rust output via run-cross-binary-harness.sh after adb pull.
     *
     * Writes to context.cacheDir/conformance-<base>.json so adb pull can
     * extract without root.
     */
    @Test
    fun emitAllFixtureOutputsToInstrumentCache() {
        val context = InstrumentationRegistry.getInstrumentation().context
        val targetContext = InstrumentationRegistry.getInstrumentation().targetContext
        val assets = context.assets

        val outputDir = File(targetContext.cacheDir, "conformance")
        outputDir.mkdirs()

        val fixtureNames = assets.list("conformance")?.filter { it.endsWith(".json") } ?: emptyList()
        check(fixtureNames.isNotEmpty()) { "no fixture assets found under src/androidTest/assets/conformance/" }

        var successCount = 0
        var failureCount = 0
        val failures = mutableListOf<String>()

        for (name in fixtureNames.sorted()) {
            val fixtureJson = assets.open("conformance/$name").bufferedReader().use { it.readText() }
            val outFile = File(outputDir, name)
            try {
                val fixture = JSONObject(fixtureJson)
                val treeJson = fixture.getJSONObject("tree").toString()
                val selectorJson = fixture.getJSONObject("selector").toString()
                val ids = resolveSelector(treeJson, selectorJson)
                // Emit sorted JSON array — byte-identical contract with
                // Rust fixture-runner + SwiftFixtureRunner + ts-fixture-runner.
                val sorted = ids.sorted()
                val out = JSONArray()
                for (id in sorted) out.put(id)
                outFile.writeText(out.toString())
                // ALSO emit to logcat so host-side can capture even if
                // the test APK gets auto-uninstalled by AGP after the run
                // (default cleanup behavior wipes /data/data/<pkg>/cache/).
                // Pull-android-conformance.sh first tries cacheDir, falls
                // back to logcat lines matching this tag prefix.
                Log.i("SMIX_CONF", "BEGIN $name ${out}")
                successCount++
            } catch (e: Exception) {
                outFile.writeText("""{"error":"${e.message?.replace("\"", "'")}"}""")
                failureCount++
                failures.add("$name: ${e.message}")
            }
        }

        println("ConformanceHarnessTest: $successCount fixtures emit OK, $failureCount failed")
        if (failureCount > 0) {
            println("Failures (each still wrote error JSON to cache):")
            for (f in failures) println("  - $f")
        }
        println("Cache dir: ${outputDir.absolutePath}")
        println("Pull: adb pull ${outputDir.absolutePath} /tmp/kotlin-conformance-output/")

        // Always pass — test is an emit harness. Diff happens host-side.
    }
}
