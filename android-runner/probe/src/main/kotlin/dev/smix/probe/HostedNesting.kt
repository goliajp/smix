package dev.smix.probe

/**
 * Where a hosted View goes in the semantics tree: under the Compose node
 * that holds it, beneath that node's own children.
 *
 * `AndroidView` content has no semantics node, and the probe used to hang
 * every hosted View off the root, after all of the root's children. A
 * reader that takes the later of two siblings as the one on top — which
 * is how a touch's target is found — then put a video player over the
 * card that contains it and over the button drawn on the player, and a
 * consumer's taps on both were reported as misses while the app did what
 * they asked.
 *
 * The holder is found by where things are: the deepest node whose
 * rectangle contains the View's, the smallest where siblings both do.
 * Beneath its other children because Compose draws what it composes after
 * the `AndroidView` over it, and a node that the View covered would not
 * be seen to put there.
 */
internal object HostedNesting {
    fun nest(root: ProbeNode, hosted: List<ProbeNode>): ProbeNode =
        hosted.fold(root) { tree, view -> insert(tree, view) }

    private fun insert(node: ProbeNode, view: ProbeNode): ProbeNode {
        val holder = node.children.withIndex()
            .filter { contains(it.value.bounds, view.bounds) }
            .minByOrNull { area(it.value.bounds) }
            ?: return node.copy(children = listOf(view) + node.children)
        val children = node.children.toMutableList()
        children[holder.index] = insert(holder.value, view)
        return node.copy(children = children)
    }

    private fun contains(outer: Bounds, inner: Bounds): Boolean =
        outer.left <= inner.left && outer.top <= inner.top &&
            outer.right >= inner.right && outer.bottom >= inner.bottom &&
            area(outer) > 0

    private fun area(b: Bounds): Long =
        (b.right - b.left).toLong().coerceAtLeast(0) * (b.bottom - b.top).toLong().coerceAtLeast(0)
}
