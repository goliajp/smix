package dev.smix.probe

import dev.smix.probe.WindowSiblings.Drawn
import org.junit.Assert.assertEquals
import org.junit.Test

class WindowSiblingsTest {
    private fun node(name: String, children: List<ProbeNode> = emptyList()) = interopNode(
        resourceId = name,
        contentDescription = null,
        text = name,
        hint = null,
        className = "android.view.View",
        layout = Bounds(0, 0, 100, 100),
        clip = Bounds(0, 0, 100, 100),
        placed = true,
        focused = false,
        enabled = true,
        children = children,
        touch = null,
    )!!

    private fun names(n: ProbeNode): List<String?> = n.children.map { it.text }

    @Test
    fun aViewDrawnOverTheComposeRootIsItsLastChild() {
        val root = node("compose", listOf(node("button")))
        val out = WindowSiblings.attach(listOf(root), listOf(Drawn.Root(0), Drawn.Beside(node("badge"))))
        assertEquals(1, out.size)
        assertEquals(listOf("button", "badge"), names(out[0]))
    }

    @Test
    fun aViewDrawnBeneathComesBeforeTheRootsOwnChildren() {
        val root = node("compose", listOf(node("button")))
        val out = WindowSiblings.attach(listOf(root), listOf(Drawn.Beside(node("backdrop")), Drawn.Root(0)))
        assertEquals(listOf("backdrop", "button"), names(out[0]))
    }

    @Test
    fun rootsOfOneWindowComeOutInDrawingOrderEachWithWhatIsOverIt() {
        val a = node("a")
        val b = node("b")
        val out = WindowSiblings.attach(
            listOf(a, b),
            listOf(Drawn.Root(1), Drawn.Beside(node("over-b")), Drawn.Root(0), Drawn.Beside(node("over-a"))),
        )
        assertEquals(listOf("b", "a"), out.map { it.text })
        assertEquals(listOf("over-b"), names(out[0]))
        assertEquals(listOf("over-a"), names(out[1]))
    }

    @Test
    fun aRootTheWalkDidNotReachKeepsItsPlaceAfterTheOthers() {
        val out = WindowSiblings.attach(listOf(node("a"), node("b")), listOf(Drawn.Root(1)))
        assertEquals(listOf("b", "a"), out.map { it.text })
    }

    @Test
    fun aWalkThatFoundNoRootChangesNothing() {
        val roots = listOf(node("a"))
        assertEquals(roots, WindowSiblings.attach(roots, listOf(Drawn.Beside(node("x")))))
    }
}
