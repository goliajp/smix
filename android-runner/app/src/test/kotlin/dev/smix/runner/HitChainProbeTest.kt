// What is under a point on a Compose screen, from the reader that aimed.
//
// Measured on the fixture's Compose screen (2026-09-27): for 150-300 ms
// after the screen comes in, the semantics probe already has
// `compose_input` and the accessibility projection does not yet. A tap
// aimed from the probe in that gap focused the field every time (8 of 8)
// while the chain, read from the projection, held only the containers —
// and the host reported the tap as a miss.

package dev.smix.runner

import org.json.JSONArray
import org.junit.Assert.assertEquals
import org.junit.Test

class HitChainProbeTest {
    private val display = HitChain.Box(0, 0, 1080, 2340)

    // The projection mid-entrance: the Compose column is there, its fields
    // are not yet.
    private val appWindow = HitChain.Window(
        layer = 1,
        bounds = display,
        root = HitChain.Node(
            "", "", HitChain.Box(0, 0, 1080, 2340),
            listOf(
                HitChain.Node(
                    "content", "", HitChain.Box(0, 136, 1080, 2208),
                    listOf(HitChain.Node("", "", HitChain.Box(44, 180, 1036, 1392), emptyList())),
                ),
            ),
        ),
        pkg = "dev.smix.fixture",
        application = true,
    )

    private val statusBar = HitChain.Window(
        layer = 5,
        bounds = HitChain.Box(0, 0, 1080, 136),
        root = HitChain.Node("", "", HitChain.Box(0, 0, 1080, 136), emptyList()),
        pkg = "com.android.systemui",
    )

    private val probe = HitChain.nodesFromProbe(
        JSONArray(
            """[{"id":1,"bounds":[0,136,1080,1436],"visibleBounds":[0,136,1080,1436],"children":[
                 {"id":2,"bounds":[44,180,1036,1392],"visibleBounds":[44,180,1036,1392],"children":[
                   {"id":4,"testTag":"compose_input","bounds":[44,225,814,379],"visibleBounds":[44,225,814,379],"children":[]},
                   {"id":9,"resourceId":"hosted_button","contentDescription":"Hosted","bounds":[44,1300,300,1390],"visibleBounds":[44,1300,300,1390],"children":[]},
                   {"id":7,"testTag":"row_far_below","bounds":[44,2400,1036,2500],"visibleBounds":[0,0,0,0],"children":[]}
                 ]}]}]""",
        ),
    )

    @Test
    fun a_field_the_probe_has_is_under_the_point_while_the_projection_catches_up() {
        val windows = HitChain.withProbe(listOf(appWindow, statusBar), "dev.smix.fixture", probe, display)
        val chain = HitChain.at(windows, 429, 302)
        assertEquals("compose_input", chain.first().id)
    }

    @Test
    fun a_window_of_another_app_above_the_point_still_takes_the_touch() {
        val windows = HitChain.withProbe(listOf(appWindow, statusBar), "dev.smix.fixture", probe, display)
        assertEquals(HitChain.Box(0, 0, 1080, 136), HitChain.at(windows, 429, 60).first().bounds)
    }

    @Test
    fun a_hosted_view_is_named_by_its_resource_id_and_description() {
        val windows = HitChain.withProbe(listOf(appWindow), "dev.smix.fixture", probe, display)
        val hit = HitChain.at(windows, 100, 1350).first()
        assertEquals("hosted_button" to "Hosted", hit.id to hit.label)
    }

    @Test
    fun a_node_placed_off_screen_is_under_no_point() {
        val windows = HitChain.withProbe(listOf(appWindow), "dev.smix.fixture", probe, display)
        assertEquals(emptyList<String>(), HitChain.at(windows, 500, 2450).map { it.id }.filter { it.isNotEmpty() })
    }

    @Test
    fun the_app_listed_by_no_window_is_read_from_the_probe_under_the_rest() {
        val windows = HitChain.withProbe(listOf(statusBar), "dev.smix.fixture", probe, display)
        assertEquals("compose_input", HitChain.at(windows, 429, 302).firstOrNull()?.id)
    }

    private val noField = HitChain.nodesFromProbe(
        JSONArray("""[{"id":1,"bounds":[0,136,1080,1436],"visibleBounds":[0,136,1080,1436],"children":[]}]"""),
    )

    private fun ids(r: HitChain.Reading): List<String> = when (r) {
        is HitChain.Reading.Read -> r.chain.map { it.id }
        is HitChain.Reading.Unreadable -> error("unreadable: ${r.why}")
    }

    @Test
    fun the_reader_that_aimed_finds_the_field_and_the_other_does_not() {
        val windows = listOf(appWindow, statusBar)
        val sem = HitChain.read(HitChain.Reader.SEMANTICS, windows, 429, 302, "dev.smix.fixture", probe, display)
        val acc = HitChain.read(HitChain.Reader.ACCESSIBILITY, windows, 429, 302, "dev.smix.fixture", probe, display)
        assertEquals(true, "compose_input" in ids(sem))
        assertEquals(false, "compose_input" in ids(acc))
    }

    @Test
    fun a_field_neither_reader_has_is_not_found_by_either() {
        val r = HitChain.read(HitChain.Reader.SEMANTICS, listOf(appWindow), 429, 302, "dev.smix.fixture", noField, display)
        assertEquals(false, "compose_input" in ids(r))
    }

    @Test
    fun a_probe_that_did_not_answer_is_said_and_not_replaced() {
        val r = HitChain.read(HitChain.Reader.SEMANTICS, listOf(appWindow), 429, 302, "dev.smix.fixture", null, display)
        assertEquals(true, r is HitChain.Reading.Unreadable)
        val body = org.json.JSONObject(RunnerWire.tapAtNormCoordBody(true, 1080, 2340, 429, 302, r))
        assertEquals(0, body.getJSONArray("chain").length())
        assertEquals(false, body.getBoolean("complete"))
        assertEquals("dev.smix.fixture's probe did not answer", body.getString("readerError"))
        assertEquals(false, body.has("reader"))
    }

    @Test
    fun the_answer_names_the_reader_it_read_and_the_request_names_the_one_it_aimed_from() {
        val r = HitChain.read(HitChain.Reader.SEMANTICS, listOf(appWindow), 429, 302, "dev.smix.fixture", probe, display)
        assertEquals("semantics", org.json.JSONObject(RunnerWire.tapAtNormCoordBody(true, 1080, 2340, 429, 302, r)).getString("reader"))
        assertEquals(HitChain.Reader.SEMANTICS, RunnerWire.decodeAimedBy("""{"nx":0.4,"ny":0.1,"aimedBy":"semantics"}"""))
        assertEquals(HitChain.Reader.ACCESSIBILITY, RunnerWire.decodeAimedBy("""{"nx":0.4,"ny":0.1}"""))
    }
}
