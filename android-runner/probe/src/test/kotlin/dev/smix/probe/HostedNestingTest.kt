package dev.smix.probe

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * A consumer's camera list: a card, the player an `AndroidView` hosts
 * inside it, and a walkthrough button Compose draws over the player.
 */
class HostedNestingTest {
    private fun node(
        tag: String?,
        b: Bounds,
        children: List<ProbeNode> = emptyList(),
        resourceId: String? = null,
    ) = ProbeNode(
        id = 0,
        testTag = tag,
        resourceId = resourceId,
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
    private val cardBox = Bounds(42, 1251, 1038, 1874)
    private val buttonBox = Bounds(300, 1500, 800, 1600)
    private val player = node(null, Bounds(42, 1251, 1038, 1874), resourceId = "view_player_status")
    private val header = node("header", Bounds(0, 0, 1080, 200))
    private val card = node("feed-camera-cam-hallway", cardBox, listOf(node("walkthrough-advance", buttonBox)))

    private fun tags(n: ProbeNode): List<String> = n.children.map { it.testTag ?: it.resourceId ?: "?" }

    @Test
    fun a_hosted_view_goes_under_the_node_that_holds_it_beneath_its_children() {
        val nested = HostedNesting.nest(node("root", screen, listOf(header, card)), listOf(player))
        assertEquals(listOf("header", "feed-camera-cam-hallway"), tags(nested))
        assertEquals(listOf("view_player_status", "walkthrough-advance"), tags(nested.children[1]))
    }

    @Test
    fun of_two_siblings_that_hold_it_the_smaller_does() {
        val outer = node("list", Bounds(0, 1000, 1080, 2340))
        val nested = HostedNesting.nest(node("root", screen, listOf(outer, card)), listOf(player))
        assertEquals(listOf("view_player_status", "walkthrough-advance"), tags(nested.children[1]))
        assertEquals(emptyList<String>(), tags(nested.children[0]))
    }

    @Test
    fun a_view_no_node_holds_goes_under_the_root_beneath_everything() {
        val wide = node(null, Bounds(0, 0, 1080, 2340), resourceId = "overlay")
        val nested = HostedNesting.nest(node("root", screen, listOf(header, card)), listOf(wide))
        assertEquals(listOf("overlay", "header", "feed-camera-cam-hallway"), tags(nested))
    }
}
