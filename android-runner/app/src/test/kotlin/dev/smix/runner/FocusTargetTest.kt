// Which field did the caller mean?
//
// `/input-text` and `/clear-text` act on "the focused field". After a
// focus tap that is ambiguous: focus in Compose does not move
// synchronously with the tap, so both routes could still reach the
// field that had focus before — the characters landed in the wrong
// field and the wrong field was emptied first. Measured on
// emulator-5554: naming compose_input while compose_password held
// focus, the runner cleared compose_password (10 -> 0) and typed
// sixteen characters into it.
//
// The wait that was meant to prevent this asked whether *some* editable
// node had focus. One already did. A predicate true before the action
// it guards is not guarding it.
//
// So the request now carries where the caller tapped, and the runner
// accepts only a focused field containing that point. An absent point
// still means "wherever focus is" — that is what `inputText` with no
// selector is, and what older clients send. `absence_is_deliberate`
// below pins that as a decision rather than a hole.

package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class FocusTargetTest {
    @Test
    fun a_request_can_say_where_the_caller_tapped() {
        val req = RunnerWire.decodeInputText("""{"text":"x","focusNx":0.5,"focusNy":0.25}""")
        assertEquals("x", req.text)
        assertEquals(0.5, req.focusAt!!.nx, 1e-9)
        assertEquals(0.25, req.focusAt!!.ny, 1e-9)
    }

    @Test
    fun a_request_need_not_say_and_then_means_wherever_focus_is() {
        val req = RunnerWire.decodeInputText("""{"text":"x"}""")
        assertEquals("x", req.text)
        assertNull(req.focusAt)
    }

    @Test
    fun clear_carries_the_same_point_and_tolerates_an_empty_body() {
        assertEquals(0.5, RunnerWire.decodeClearText("""{"focusNx":0.5,"focusNy":0.9}""")!!.nx, 1e-9)
        assertNull(RunnerWire.decodeClearText("{}"))
        assertNull(RunnerWire.decodeClearText(""))
    }

    @Test
    fun a_point_inside_the_bounds_is_held() {
        assertTrue(RunnerWire.nodeHoldsPoint(44, 379, 814, 533, 429, 456))
    }

    @Test
    fun a_point_outside_the_bounds_is_not() {
        // compose_input's centre against compose_password's bounds:
        // the exact confusion this exists to stop.
        assertFalse(RunnerWire.nodeHoldsPoint(44, 379, 814, 533, 429, 302))
    }

    @Test
    fun the_edge_counts_as_inside() {
        assertTrue(RunnerWire.nodeHoldsPoint(44, 379, 814, 533, 44, 379))
        assertTrue(RunnerWire.nodeHoldsPoint(44, 379, 814, 533, 814, 533))
    }

    @Test
    fun absence_is_deliberate() {
        // No point given: any focused field is accepted, which is what
        // a bare `inputText` means and what older clients send.
        assertTrue(RunnerWire.focusAccepts(44, 379, 814, 533, null, null))
        // A point given: only the field holding it is accepted. Without
        // this half the one above would just be a hole.
        assertFalse(RunnerWire.focusAccepts(44, 379, 814, 533, 429, 302))
        assertTrue(RunnerWire.focusAccepts(44, 379, 814, 533, 429, 456))
    }
}
