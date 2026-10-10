package dev.smix.probe

/**
 * What a window the app paints opaquely hides of the app's windows below it.
 *
 * A consumer's app pushes every page as a full-screen dialog with an opaque
 * background. The page under it stays VISIBLE and laid out, so the probe
 * reported its text as on screen while nothing of it could be seen, and a
 * tap aimed at a row of the covered list landed on the page above.
 *
 * The accessibility service drops a covered window by its bounds alone, and
 * so drops a page that shows through a translucent sheet over it — which is
 * why this asks the app instead: only a window whose root View is opaque
 * hides what is under it. A node is left out when all of the part of it that
 * shows lies inside one such window above it; a node partly covered stays.
 */
object Occlusion {
    /**
     * `roots` bottom first, each with the screen rectangle its window paints
     * opaquely, or null for a window that does not.
     */
    fun prune(roots: List<Pair<Bounds?, ProbeNode>>): List<ProbeNode> =
        roots.mapIndexedNotNull { i, (_, root) ->
            val above = roots.drop(i + 1).mapNotNull { it.first }
            if (above.isEmpty()) root else hide(root, above)
        }

    private fun hide(node: ProbeNode, covers: List<Bounds>): ProbeNode? {
        val children = node.children.mapNotNull { hide(it, covers) }
        val covered = node.visible && covers.any { contains(it, node.visibleBounds) }
        return when {
            !covered -> node.copy(children = children)
            children.isEmpty() -> null
            // What it holds still shows somewhere; it does not.
            else -> node.copy(
                children = children,
                visible = false,
                visibleBounds = Bounds(
                    node.visibleBounds.left,
                    node.visibleBounds.top,
                    node.visibleBounds.left,
                    node.visibleBounds.top,
                ),
            )
        }
    }

    private fun contains(outer: Bounds, inner: Bounds): Boolean =
        inner.left >= outer.left && inner.top >= outer.top &&
            inner.right <= outer.right && inner.bottom <= outer.bottom
}
