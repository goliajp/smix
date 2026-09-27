package dev.smix.probe

import org.junit.Assert.assertEquals
import org.junit.Test

class StackOrderTest {
    private val outgoing = Any()
    private val incoming = Any()
    private val dialog = Any()

    @Test
    fun the_window_added_last_is_listed_last() {
        // The weak set handed the incoming activity's root over first.
        val roots = listOf("incoming" to incoming, "outgoing" to outgoing)
        val ordered = StackOrder.bottomFirst(roots, { it.second }, listOf(outgoing, incoming))
        assertEquals(listOf("outgoing", "incoming"), ordered.map { it.first })
    }

    @Test
    fun a_dialog_stands_over_its_activity() {
        val roots = listOf("dialog" to dialog, "activity" to incoming)
        val ordered = StackOrder.bottomFirst(roots, { it.second }, listOf(incoming, dialog))
        assertEquals(listOf("activity", "dialog"), ordered.map { it.first })
    }

    @Test
    fun a_root_whose_window_is_not_listed_goes_to_the_bottom() {
        val roots = listOf("listed" to incoming, "gone" to Any())
        val ordered = StackOrder.bottomFirst(roots, { it.second }, listOf(incoming))
        assertEquals(listOf("gone", "listed"), ordered.map { it.first })
    }
}
