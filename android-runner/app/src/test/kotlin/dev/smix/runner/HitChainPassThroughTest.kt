// A layer drawn over a control that does nothing with a touch lets it
// through to the control.
//
// A consumer's camera page (2026-10-12): the detection boxes are a
// full-size Compose layer over the player, composed after the "GO LIVE"
// pill. The tap at the pill went back to live; the chain, taking the
// later sibling as the one on top, went into the boxes and the host
// reported a miss.

package dev.smix.runner

import org.json.JSONArray
import org.junit.Assert.assertEquals
import org.junit.Test

class HitChainPassThroughTest {
    private val display = HitChain.Box(0, 0, 1080, 2400)

    private fun player(overlay: String) = HitChain.nodesFromProbe(
        JSONArray(
            """[{"id":1,"bounds":[0,0,1080,2400],"visibleBounds":[0,0,1080,2400],"actions":[],"children":[
                 {"id":2,"bounds":[0,258,1080,1068],"visibleBounds":[0,258,1080,1068],"actions":[],"children":[
                   {"id":3,"testTag":"btn_player_mode_go_live","bounds":[42,960,210,1050],"visibleBounds":[42,960,210,1050],"actions":["OnClick"],"children":[]},
                   $overlay
                 ]}]}]""",
        ),
    )

    private fun firstId(roots: List<HitChain.Node>, x: Int, y: Int): String =
        HitChain.at(HitChain.withProbe(emptyList(), "app", roots, display), x, y).first().id

    @Test
    fun boxes_drawn_over_the_pill_let_the_tap_through_to_it() {
        val boxes = """{"id":4,"testTag":"box-detection","bounds":[0,258,1080,1068],"visibleBounds":[0,258,1080,1068],"actions":[],"children":[]}"""
        assertEquals("btn_player_mode_go_live", firstId(player(boxes), 126, 1005))
    }

    @Test
    fun a_scrim_that_takes_the_tap_still_stands_over_the_pill() {
        val scrim = """{"id":4,"testTag":"walkthrough-scrim","bounds":[0,258,1080,1068],"visibleBounds":[0,258,1080,1068],"actions":["OnClick"],"children":[]}"""
        assertEquals("walkthrough-scrim", firstId(player(scrim), 126, 1005))
    }

    @Test
    fun a_view_hosted_over_the_pill_is_taken_to_stop_the_tap() {
        val view = """{"id":4,"resourceId":"player_surface","className":"android.view.SurfaceView","bounds":[0,258,1080,1068],"visibleBounds":[0,258,1080,1068],"actions":[],"children":[]}"""
        assertEquals("player_surface", firstId(player(view), 126, 1005))
    }

    @Test
    fun where_nothing_takes_a_touch_the_topmost_is_still_the_answer() {
        val boxes = """{"id":4,"testTag":"box-detection","bounds":[0,258,1080,1068],"visibleBounds":[0,258,1080,1068],"actions":[],"children":[]}"""
        // A point beside the pill: only the boxes and the stage hold it.
        assertEquals("box-detection", firstId(player(boxes), 700, 500))
    }
}
