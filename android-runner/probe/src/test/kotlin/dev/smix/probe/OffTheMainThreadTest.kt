package dev.smix.probe

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Nothing in the probe asks a View to populate an accessibility node.
 *
 * The provider answers on binder threads. A ViewGroup populating its node
 * lists its children into a scratch list its window shares with the main
 * thread, so doing it here races the app answering the accessibility
 * service and crashes the app under test — a probe in an app is never
 * allowed to do that. Constructing the node around the View, which only
 * reads ids, is what the probe does instead.
 */
class OffTheMainThreadTest {
    private val sources = File("src/main/kotlin").walkTopDown().filter { it.extension == "kt" }.toList()

    @Test
    fun `the probe's sources are where this looks`() {
        assertTrue("no Kotlin sources under ${File("src/main/kotlin").absolutePath}", sources.size >= 5)
        assertTrue(sources.any { it.readText().contains("AccessibilityNodeInfo(it)") })
    }

    @Test
    fun `no View is asked to populate an accessibility node`() {
        val offenders = sources.flatMap { f ->
            f.readLines().withIndex()
                .filter { (_, line) -> line.contains("createAccessibilityNodeInfo(") || line.contains("onInitializeAccessibilityNodeInfo(") }
                .map { (i, line) -> "${f.name}:${i + 1}: ${line.trim()}" }
        }
        assertEquals("populated off the main thread", emptyList<String>(), offenders)
    }

    @Test
    fun `the semantics tree is walked from one place, and that place runs on the main thread`() {
        // A walk from the binder thread the question arrives on meets layout
        // passes half done, and can end the app; see MainThread.
        val calls = sources.flatMap { f ->
            f.readLines().withIndex()
                .filter { (_, line) -> line.contains("dumpWireJson()") && !line.contains("fun dumpWireJson()") }
                .map { (i, line) -> "${f.name}:${i + 1}: ${line.trim()}" }
        }
        assertEquals(1, calls.size)
        assertTrue(calls.single(), calls.single().contains("Reading(dumpWireJson(), w, h"))
        val probe = sources.single { it.name == "SemanticsProbe.kt" }.readText()
        assertTrue(probe.contains("MainThread.run(waitMs, { !hasPendingLayout() }, ::readHere)"))
    }
}
