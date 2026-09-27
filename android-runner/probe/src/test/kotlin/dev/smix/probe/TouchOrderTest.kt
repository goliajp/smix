package dev.smix.probe

import org.junit.Assert.assertEquals
import org.junit.Test

class TouchOrderTest {
    @Test
    fun a_group_that_draws_its_first_child_last_puts_it_on_top() {
        // The controls are child 0 and are drawn after the full-size layer.
        val order = TouchOrder.bottomFirst(2, { p -> 1 - p }, { 0f })
        assertEquals(listOf(1, 0), order)
    }

    @Test
    fun a_group_that_keeps_index_order_is_read_as_before() {
        assertEquals(listOf(0, 1, 2), TouchOrder.bottomFirst(3, { it }, { 0f }))
    }

    @Test
    fun elevation_comes_before_drawing_position() {
        val z = floatArrayOf(4f, 0f)
        assertEquals(listOf(1, 0), TouchOrder.bottomFirst(2, { it }, { z[it] }))
    }
}
