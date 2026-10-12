// Where a touch goes, read from what each layer does with one.
//
// Shapes trimmed from a consumer's probe trees, read the moment before
// five taps the app answered and smix reported as missed (2026-10-12,
// 1080x2400 emulator): a Material bottom sheet whose panel sits below the
// container holding it; a polygon vertex handle that drags with
// `pointerInput` and has no semantics action; a player whose controls sit
// under full-size Views that are not clickable; a camera's name label over
// the card it opens.

package dev.smix.runner

import org.json.JSONArray
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class HitChainTouchTest {
    private val display = HitChain.Box(0, 0, 1080, 2400)

    private fun find(tree: String, x: Int, y: Int): HitChain.Found =
        HitChain.find(HitChain.withProbe(emptyList(), "app", HitChain.nodesFromProbe(JSONArray(tree)), display), x, y)

    private fun ids(f: HitChain.Found) = f.entries.map { it.id.ifEmpty { it.label } }

    private fun sheet(touch: Boolean): String {
        fun t(v: String) = if (touch) ""","touch":"$v"""" else ""
        return """[{"id":511,"bounds":[0,0,1080,2400],"visibleBounds":[0,0,1080,2400],"actions":[]${t("through")},"children":[
          {"id":512,"contentDescription":"Close sheet","bounds":[0,0,1080,2400],"visibleBounds":[0,0,1080,2400],"actions":["OnClick"]${t("takes")},"children":[]},
          {"id":513,"bounds":[0,0,1080,1015],"visibleBounds":[0,0,1080,1015],"actions":[]${t("through")},"children":[
            {"id":514,"bounds":[0,1385,1080,2337],"visibleBounds":[0,1385,1080,2337],"actions":[]${t("takes")},"children":[
              {"id":523,"testTag":"btn-snooze-30","bounds":[0,1639,1080,1766],"visibleBounds":[0,1639,1080,1766],"actions":["OnClick"]${t("takes")},"children":[]},
              {"id":524,"testTag":"btn-snooze-60","bounds":[0,1766,1080,1893],"visibleBounds":[0,1766,1080,1893],"actions":["OnClick"]${t("takes")},"children":[]}
            ]}]}]}]"""
    }

    @Test
    fun a_sheet_panel_below_its_container_takes_the_press_of_its_button() {
        assertEquals("btn-snooze-30", ids(find(sheet(touch = true), 540, 1702)).first())
    }

    @Test
    fun a_probe_that_does_not_say_how_touches_go_still_reaches_past_the_container_edge() {
        assertEquals("btn-snooze-30", ids(find(sheet(touch = false), 540, 1702)).first())
    }

    @Test
    fun the_scrim_still_takes_a_press_beside_the_sheet() {
        assertEquals("Close sheet", ids(find(sheet(touch = true), 540, 600)).first())
    }

    private val canvas = """[{"id":815,"testTag":"canvas-zones","bounds":[0,268,1080,876],"visibleBounds":[0,268,1080,876],"actions":[],"touch":"through","children":[
      {"id":816,"bounds":[0,268,1080,876],"visibleBounds":[0,268,1080,876],"actions":[],"touch":"through","children":[]},
      {"id":850,"testTag":"zone-0","bounds":[378,481,702,663],"visibleBounds":[378,481,702,663],"actions":["OnClick"],"touch":"takes","children":[]},
      {"id":851,"testTag":"zone-selected-point-0","bounds":[315,418,441,544],"visibleBounds":[315,418,441,544],"actions":[],"touch":"takes","children":[]}
    ]}]"""

    @Test
    fun a_handle_that_only_drags_takes_the_touch_from_the_zone_under_it() {
        assertEquals("zone-selected-point-0", ids(find(canvas, 378, 481)).first())
    }

    private fun player(overlayTouch: String) = """[{"id":594,"bounds":[0,258,1080,1068],"visibleBounds":[0,258,1080,1068],"actions":[],"touch":"takes","children":[
      {"id":-1,"className":"com.example.PlayerHost","bounds":[0,258,1080,1068],"visibleBounds":[0,258,1080,1068],"actions":[],"touch":"through","children":[
        {"id":-1,"className":"androidx.media3.ui.PlayerView","bounds":[0,258,1080,1068],"visibleBounds":[0,258,1080,1068],"actions":[],"touch":"maybe","children":[
          {"id":-1,"className":"com.example.ModeSwitcher","bounds":[0,258,1080,1068],"visibleBounds":[0,258,1080,1068],"actions":[],"touch":"through","children":[
            {"id":-1,"resourceId":"btn_player_mode_go_live","className":"com.example.ModePill","bounds":[41,942,212,1068],"visibleBounds":[41,942,212,1068],"actions":[],"touch":"takes","children":[]}
          ]},
          {"id":-1,"resourceId":"exo_overlay","className":"android.widget.FrameLayout","bounds":[0,258,1080,1068],"visibleBounds":[0,258,1080,1068],"actions":[],"touch":"through","children":[]}
        ]},
        {"id":-1,"resourceId":"overlay_detections","className":"com.example.DetectionOverlay","bounds":[0,258,1080,1068],"visibleBounds":[0,258,1080,1068],"actions":[],"touch":"$overlayTouch","children":[]},
        {"id":-1,"className":"com.example.TransportBar","bounds":[0,258,1080,1068],"visibleBounds":[0,258,1080,1068],"actions":[],"touch":"through","children":[
          {"id":-1,"className":"android.widget.LinearLayout","bounds":[0,258,1080,1068],"visibleBounds":[0,258,1080,1068],"actions":[],"touch":"through","children":[]}
        ]}
      ]}]}]"""

    @Test
    fun views_that_are_not_clickable_let_the_touch_through_to_the_pill() {
        val f = find(player("through"), 126, 1005)
        assertEquals("btn_player_mode_go_live", ids(f).first())
        assertNull(f.uncertain)
        // the layers it crossed are under the point too
        assertTrue("overlay_detections" in ids(f))
    }

    @Test
    fun a_view_whose_own_code_decides_is_named_as_the_reason_the_answer_is_not_certain() {
        val f = find(player("maybe"), 126, 1005)
        assertEquals("overlay_detections", ids(f).first())
        val why = f.uncertain
        assertNotNull(why)
        assertTrue(why!!, why.contains("id=overlay_detections"))
    }

    private val card = """[{"id":127,"testTag":"feed-camera-1","bounds":[42,773,1038,1396],"visibleBounds":[42,773,1038,1396],"actions":["OnClick"],"touch":"takes","children":[
      {"id":317,"bounds":[42,773,1038,1396],"visibleBounds":[42,773,1038,1396],"actions":[],"touch":"takes","children":[]},
      {"id":128,"bounds":[89,810,194,852],"visibleBounds":[89,810,194,852],"actions":[],"touch":"through","children":[
        {"id":129,"contentDescription":"Kitchen","bounds":[89,810,194,852],"visibleBounds":[89,810,194,852],"actions":[],"touch":"through","children":[]}
      ]}
    ]}]"""

    @Test
    fun a_label_the_touch_goes_through_is_under_the_point_with_the_card_it_opens() {
        val f = find(card, 141, 831)
        assertTrue(ids(f).toString(), "feed-camera-1" in ids(f))
        assertTrue(ids(f).toString(), "Kitchen" in ids(f))
        assertTrue(f.entries.any { it.bounds == HitChain.Box(89, 810, 194, 852) && it.id.isEmpty() && it.label.isEmpty() })
    }

    @Test
    fun a_clickable_layer_over_the_label_is_not_passed_through() {
        val covered = """[{"id":127,"testTag":"feed-camera-1","bounds":[42,773,1038,1396],"visibleBounds":[42,773,1038,1396],"actions":["OnClick"],"touch":"takes","children":[
          {"id":128,"bounds":[89,810,194,852],"visibleBounds":[89,810,194,852],"actions":[],"touch":"through","children":[
            {"id":129,"contentDescription":"Kitchen","bounds":[89,810,194,852],"visibleBounds":[89,810,194,852],"actions":[],"touch":"through","children":[]}
          ]},
          {"id":901,"testTag":"top-scrim","bounds":[42,773,1038,1396],"visibleBounds":[42,773,1038,1396],"actions":["OnClick"],"touch":"takes","children":[]}
        ]}]"""
        val f = find(covered, 141, 831)
        assertEquals("top-scrim", ids(f).first())
        assertTrue(ids(f).toString(), "Kitchen" !in ids(f))
    }

    // A consumer's zone editor: polygons drawn on a canvas, a gesture layer
    // with no semantics that decides by the polygon, and over it one box per
    // zone on the polygon's bounding rectangle carrying only semantics
    // { onClick }. Two zones' rectangles overlap; the tap is in zone-0's
    // polygon.
    private val zones = """[{"id":1017,"testTag":"canvas-zones","bounds":[0,268,1080,876],"visibleBounds":[0,268,1080,876],"actions":[],"touch":"through","children":[
      {"id":1045,"bounds":[0,268,1080,876],"visibleBounds":[0,268,1080,876],"actions":[],"touch":"through","children":[]},
      {"id":1044,"testTag":"zone-0","bounds":[378,481,702,663],"visibleBounds":[378,481,702,663],"actions":["OnClick"],"touch":"through","children":[]},
      {"id":1046,"testTag":"zone-1","bounds":[450,520,800,700],"visibleBounds":[450,520,800,700],"actions":["OnClick"],"touch":"through","children":[]}
    ]}]"""

    @Test
    fun overlapping_zone_boxes_that_only_offer_semantics_are_both_under_the_point() {
        val f = find(zones, 540, 572)
        assertTrue(ids(f).toString(), "zone-0" in ids(f))
        assertTrue(ids(f).toString(), "zone-1" in ids(f))
    }
}
