// Conformance fixture #1 (empty tree + id miss → []).
//
// Mirror of:
//   - crates/smix-core-conformance/fixtures/spike-001-empty-tree.json (Rust)
//   - swift-bridge/Tests/SmixCoreFFITests/Spike001Tests.swift (iOS Swift)
//
// Proves Kotlin FFI binding (UniFFI 0.29.5-generated) calls into the
// same Rust core (libsmix_ffi.so via JNA) and produces byte-identical
// output to Rust + Swift backends. Conformance T1 third cell (Rust +
// Swift + Kotlin three-way byte-identical).
//
package dev.smix.sdk

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.smix.FfiException
import uniffi.smix.resolveSelector

@RunWith(AndroidJUnit4::class)
class Spike001Test {

    /** Empty a11y tree + id selector miss → empty `List<String>`. */
    @Test
    fun testEmptyTreeIdMissByteIdenticalToRust() {
        val treeJson = """{"rawType":"other","bounds":{"x":0,"y":0,"w":393,"h":852},"enabled":true,"selected":false,"hasFocus":false,"visible":true,"children":[]}"""
        val selectorJson = """{"id":"nope"}"""

        val result = resolveSelector(treeJson, selectorJson)

        assertEquals(
            "Kotlin FFI must return [] (byte-identical to Rust + Swift backends) on empty tree + id miss",
            emptyList<String>(),
            result,
        )
    }

    /** Invalid tree JSON → throws FfiException.InvalidTreeJson. */
    @Test
    fun testInvalidTreeJsonThrows() {
        try {
            resolveSelector("not json", "{}")
            fail("expected FfiException.InvalidTreeJson, got success")
        } catch (e: FfiException.InvalidTreeJson) {
            assertTrue("error message non-empty", e.message?.isNotEmpty() ?: false)
        }
    }

    /** Invalid selector JSON → throws FfiException.InvalidSelectorJson. */
    @Test
    fun testInvalidSelectorJsonThrows() {
        val treeJson = """{"rawType":"other","bounds":{"x":0,"y":0,"w":393,"h":852},"enabled":true,"selected":false,"hasFocus":false,"visible":true,"children":[]}"""
        try {
            resolveSelector(treeJson, "not json")
            fail("expected FfiException.InvalidSelectorJson, got success")
        } catch (e: FfiException.InvalidSelectorJson) {
            assertTrue("error message non-empty", e.message?.isNotEmpty() ?: false)
        }
    }
}
