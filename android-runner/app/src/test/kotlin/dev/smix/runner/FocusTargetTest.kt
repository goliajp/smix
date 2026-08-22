// Which field did the caller mean?
//
// `/input-text` and `/clear-text` act on "the focused field". After a
// focus tap that is ambiguous: focus in Compose does not move
// synchronously with the tap, so both routes could still reach the
// field that had focus before — measured on emulator-5554, naming one
// field while another held focus cleared the wrong one and typed
// sixteen characters into it.
//
// 6.7.1 answered that with the tap point: the focused field had to
// contain it. That refused a whole shape of app. A consumer's
// hand-written Kotlin views carry the contentDescription on the layout
// around the field and nothing on the field, so a selector resolves to
// the wrapper — and a wrapper's centre sits on a label as often as on
// the input. Every fill in that app stopped working.
//
// What identifies the field is that it lies inside what was named. The
// wrapper's box contains its input; a field elsewhere on screen does
// not overlap it.

package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class FocusTargetTest {
    @Test
    fun a_request_can_say_which_box_the_caller_named() {
        val req = RunnerWire.decodeInputText(
            """{"text":"x","focusRect":[0.1,0.2,0.8,0.1]}""",
        )
        assertEquals("x", req.text)
        assertEquals(0.1, req.focusIn!!.nx, 1e-9)
        assertEquals(0.2, req.focusIn!!.ny, 1e-9)
        assertEquals(0.8, req.focusIn!!.nw, 1e-9)
        assertEquals(0.1, req.focusIn!!.nh, 1e-9)
    }

    @Test
    fun a_request_need_not_say_and_then_means_wherever_focus_is() {
        val req = RunnerWire.decodeInputText("""{"text":"x"}""")
        assertEquals("x", req.text)
        assertNull(req.focusIn)
    }

    @Test
    fun clear_carries_the_same_box_and_tolerates_an_empty_body() {
        assertEquals(
            0.5,
            RunnerWire.decodeClearText("""{"focusRect":[0.5,0.9,0.2,0.05]}""")!!.nx,
            1e-9,
        )
        assertNull(RunnerWire.decodeClearText("{}"))
        assertNull(RunnerWire.decodeClearText(""))
    }

    @Test
    fun a_field_inside_the_named_box_is_the_one() {
        // The wrapper spans 313..543; its input sits at 313..437.
        assertTrue(RunnerWire.focusAccepts(0, 313, 1080, 437, intArrayOf(0, 313, 1080, 543)))
    }

    @Test
    fun a_field_the_named_box_does_not_reach_is_not() {
        // Another field higher up the screen, and the wrapper below it.
        assertFalse(RunnerWire.focusAccepts(0, 189, 1080, 313, intArrayOf(0, 400, 1080, 543)))
    }

    @Test
    fun the_wrappers_centre_need_not_be_inside_the_field() {
        // The geometry that broke it: wrapper 313..543 has centre 428,
        // and with a tall help block the input can sit at 313..437 with
        // the centre landing below it. Overlap is what decides, not the
        // centre — a rule keyed on the centre answers no here.
        val wrapper = intArrayOf(0, 313, 1080, 813)
        val centreY = (wrapper[1] + wrapper[3]) / 2
        assertFalse(
            "the centre is outside the field, which is why a point rule failed",
            RunnerWire.nodeHoldsPoint(0, 313, 1080, 437, 540, centreY),
        )
        assertTrue(
            "the field is still inside the wrapper",
            RunnerWire.focusAccepts(0, 313, 1080, 437, wrapper),
        )
    }

    @Test
    fun absence_is_deliberate() {
        // No box given: any focused field is accepted, which is what a
        // bare `inputText` means and what an older client sends.
        assertTrue(RunnerWire.focusAccepts(0, 313, 1080, 437, null))
        // A box given: only a field it reaches. Without this half the
        // one above would just be a hole.
        assertFalse(RunnerWire.focusAccepts(0, 100, 1080, 200, intArrayOf(0, 400, 1080, 543)))
    }

    @Test
    fun boxes_that_merely_touch_do_not_overlap() {
        // Written the other way round first, and a device said no. A
        // column of fields puts one input's bottom exactly on the
        // next input's top, so counting contact as overlap accepted
        // the field below the one that was named: naming
        // compose_input while compose_password held focus typed into
        // the password field.
        assertFalse(RunnerWire.boxesOverlap(0, 0, 10, 10, 10, 10, 20, 20))
        assertTrue(RunnerWire.boxesOverlap(0, 0, 10, 10, 9, 9, 20, 20))
    }

    @Test
    fun a_field_stacked_directly_below_the_named_one_is_not_it() {
        // The measured geometry: compose_input 225..379 and
        // compose_password 379..533, sharing the edge at 379.
        assertFalse(
            RunnerWire.focusAccepts(44, 379, 814, 533, intArrayOf(44, 225, 814, 379)),
        )
        assertTrue(
            RunnerWire.focusAccepts(44, 225, 814, 379, intArrayOf(44, 225, 814, 379)),
        )
    }
}
