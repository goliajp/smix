package dev.smix.runner

/// What is under a point, as the touch about to be delivered there
/// will find it.
///
/// An Android tap used to report nothing about where it landed, and the
/// host recorded every one as "could not be judged" — which the flow
/// then counted as a pass. A consumer's dialog confirm was pressed below
/// the dialog, dismissed it, and was reported as a tap. With the chain,
/// the host compares the element it aimed at with what the touch is
/// actually delivered to, the same comparison it has made on iOS.
///
/// Read BEFORE the touch goes in. A tap that lands on a dialog's button
/// takes the dialog away, so a chain read afterwards describes the
/// screen behind it and would call every successful confirm a miss.
///
/// Every element containing the point is listed, named or not: the host
/// can only compare an unnamed target by its frame, and "not in the
/// chain" is evidence of a miss only when the chain leaves nothing out.
object HitChain {
    data class Box(val left: Int, val top: Int, val right: Int, val bottom: Int) {
        fun contains(x: Int, y: Int): Boolean = x in left until right && y in top until bottom
    }

    data class Node(val id: String, val label: String, val bounds: Box, val children: List<Node>)

    data class Window(val layer: Int, val bounds: Box, val root: Node)

    data class Entry(val id: String, val label: String, val bounds: Box)

    /// The elements under `(px, py)` in the window a touch there is
    /// delivered to, innermost first. Empty when no window holds the point.
    ///
    /// The window is the topmost one containing the point — by layer, not
    /// by its place in the list, which is not the order on screen.
    fun at(windows: List<Window>, px: Int, py: Int): List<Entry> {
        val top = windows
            .filter { it.bounds.contains(px, py) }
            .maxByOrNull { it.layer }
            ?: return emptyList()
        val outermostFirst = mutableListOf<Entry>()
        collect(top.root, px, py, outermostFirst)
        return outermostFirst.asReversed()
    }

    private fun collect(n: Node, px: Int, py: Int, into: MutableList<Entry>) {
        if (!n.bounds.contains(px, py)) return
        into.add(Entry(n.id, n.label, n.bounds))
        // One path down: siblings do not overlap in a layout that routes
        // touches, and where they do, the later one is drawn on top.
        n.children.lastOrNull { it.bounds.contains(px, py) }?.let { collect(it, px, py, into) }
    }
}
