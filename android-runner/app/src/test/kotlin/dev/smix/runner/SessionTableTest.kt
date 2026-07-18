// SessionTable bookkeeping — the pure state machine behind the
// /session/* handlers in SmixHttpServer. Clock injected so timestamps
// are deterministic.

package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SessionTableTest {

    @Test
    fun openAllocatesSequentialIds() {
        val table = SessionTable(nowMs = { 1L })
        assertEquals("sess-android-1", table.open("com.a", activated = false).sessionId)
        assertEquals("sess-android-2", table.open("com.b", activated = false).sessionId)
    }

    @Test
    fun openStampsOpenedAtFromClock() {
        val table = SessionTable(nowMs = { 42L })
        val entry = table.open("com.example.app", activated = false)
        assertEquals("com.example.app", entry.bundleId)
        assertEquals(42L, entry.openedAtMs)
    }

    @Test
    fun openWithoutActivationLeavesLastActivatedZero() {
        val table = SessionTable(nowMs = { 42L })
        assertEquals(0L, table.open("com.a", activated = false).lastActivatedAtMs)
    }

    @Test
    fun openWithActivationStampsLastActivated() {
        val table = SessionTable(nowMs = { 42L })
        assertEquals(42L, table.open("com.a", activated = true).lastActivatedAtMs)
    }

    @Test
    fun getReturnsOpenEntryAndNullForUnknown() {
        val table = SessionTable(nowMs = { 1L })
        val entry = table.open("com.a", activated = false)
        assertEquals(entry, table.get(entry.sessionId))
        assertNull(table.get("sess-android-99"))
    }

    @Test
    fun closeRemovesAndIsIdempotent() {
        val table = SessionTable(nowMs = { 1L })
        val entry = table.open("com.a", activated = false)
        assertTrue(table.close(entry.sessionId))
        assertNull(table.get(entry.sessionId))
        assertFalse(table.close(entry.sessionId))
        assertFalse(table.close("never-existed"))
    }

    @Test
    fun closeAllReportsCountAndEmpties() {
        val table = SessionTable(nowMs = { 1L })
        table.open("com.a", activated = false)
        table.open("com.b", activated = false)
        assertEquals(2, table.closeAll())
        assertEquals(0, table.list().size)
        assertEquals(0, table.closeAll())
    }

    @Test
    fun listPreservesOpenOrder() {
        val table = SessionTable(nowMs = { 1L })
        table.open("com.a", activated = false)
        table.open("com.b", activated = false)
        assertEquals(listOf("com.a", "com.b"), table.list().map { it.bundleId })
    }

    @Test
    fun renewActivationStampsNewTimestamp() {
        var now = 100L
        val table = SessionTable(nowMs = { now })
        val entry = table.open("com.a", activated = true)
        assertEquals(100L, entry.lastActivatedAtMs)
        now = 250L
        val renewed = table.renewActivation(entry.sessionId)!!
        assertEquals(250L, renewed.lastActivatedAtMs)
        assertEquals(100L, renewed.openedAtMs)
        // The table itself observes the update, not just the returned copy.
        assertEquals(250L, table.get(entry.sessionId)!!.lastActivatedAtMs)
    }

    @Test
    fun renewActivationUnknownReturnsNull() {
        val table = SessionTable(nowMs = { 1L })
        assertNull(table.renewActivation("sess-android-7"))
    }
}
