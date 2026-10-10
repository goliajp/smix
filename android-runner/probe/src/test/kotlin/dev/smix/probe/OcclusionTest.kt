package dev.smix.probe

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/**
 * A consumer's pages are full-screen dialogs with an opaque background; the
 * page under one stays laid out and was reported as on screen.
 */
class OcclusionTest {
    private fun node(tag: String, b: Bounds, children: List<ProbeNode> = emptyList()) = ProbeNode(
        id = 0,
        testTag = tag,
        resourceId = null,
        text = null,
        hint = null,
        editableText = null,
        inputText = null,
        contentDescription = null,
        role = null,
        className = null,
        bounds = b,
        visibleBounds = b,
        focused = false,
        enabled = true,
        visible = true,
        actions = emptyList(),
        children = children,
    )

    private val screen = Bounds(0, 0, 1080, 2340)
    private val activity = node("activity", screen, listOf(node("row", Bounds(0, 400, 1080, 560))))
    private val page = node("page", screen, listOf(node("result", Bounds(0, 300, 1080, 400))))

    private fun tags(n: ProbeNode): List<String> = listOf(n.testTag!!) + n.children.flatMap(::tags)

    @Test
    fun a_page_painted_over_the_whole_screen_hides_the_one_under_it() {
        val roots = Occlusion.prune(listOf(screen to activity, screen to page))
        assertEquals(listOf(listOf("page", "result")), roots.map(::tags))
    }

    @Test
    fun a_translucent_window_hides_nothing() {
        // A sheet at the bottom of a full-screen translucent window: the page
        // shows through everywhere the sheet does not paint.
        val roots = Occlusion.prune(listOf(screen to page, null to node("sheet", screen)))
        assertEquals(listOf(listOf("page", "result"), listOf("sheet")), roots.map(::tags))
    }

    @Test
    fun a_node_partly_outside_the_window_above_stays() {
        val panel = Bounds(0, 2000, 1080, 2340)
        val roots = Occlusion.prune(listOf(screen to activity, panel to node("panel", panel)))
        assertEquals(listOf("activity", "row"), tags(roots[0]))
    }

    @Test
    fun a_covered_node_whose_child_shows_elsewhere_stays_as_not_visible() {
        val cover = Bounds(0, 0, 1080, 1000)
        val holder = node("holder", Bounds(0, 0, 1080, 900), listOf(node("spill", Bounds(0, 800, 1080, 1200))))
        val roots = Occlusion.prune(listOf(screen to node("activity", screen, listOf(holder)), cover to node("top", cover)))
        val kept = roots[0].children.single()
        assertEquals(listOf("holder", "spill"), tags(kept))
        assertFalse("its own face is covered", kept.visible)
    }
}
