package dev.smix.probe

import android.view.View
import android.view.ViewGroup
import androidx.compose.ui.platform.ViewRootForTest

/**
 * The Views a window draws beside its Compose roots.
 *
 * A window that holds a `ComposeView` was reported as that root's
 * semantics and the Views it hosts, and nothing else: the window was
 * taken as answered. A View the app put beside the Compose content —
 * an environment badge added to `android.R.id.content` over a screen
 * built in Compose — was in neither, so a flow could not find it while
 * the accessibility reader listed its text. Reported 2026-10-29 by a
 * consumer whose app rebuilds its root with a `ComposeView` in it.
 *
 * The window is walked as Views, down through the ancestors of its
 * Compose roots; every other subtree is one item, in drawing order, and
 * each Compose root marks its own place. [attach] then hangs the items
 * on the roots: an item drawn before the first root is beneath it, an
 * item drawn after root k and before the next is over root k. Items go
 * under a root rather than becoming roots of their own, for the reason
 * [hostedViews] gives — a root the size of the window would be read as
 * a dialog covering everything else.
 */
internal object WindowSiblings {
    sealed interface Drawn {
        /** The Compose root at this index of the roots handed to [attach]. */
        data class Root(val index: Int) : Drawn

        /** A View subtree outside every Compose root. */
        data class Beside(val node: ProbeNode) : Drawn
    }

    /**
     * `roots` in the order `drawn` puts them, each carrying the Views drawn
     * next to it. A root `drawn` does not mention keeps its place after the
     * ones it does.
     */
    fun attach(roots: List<ProbeNode>, drawn: List<Drawn>): List<ProbeNode> {
        val order = drawn.filterIsInstance<Drawn.Root>().map { it.index }.filter { it in roots.indices }.distinct()
        if (order.isEmpty()) return roots
        val below = mutableListOf<ProbeNode>()
        val over = mutableMapOf<Int, MutableList<ProbeNode>>()
        var current: Int? = null
        for (d in drawn) {
            when (d) {
                is Drawn.Root -> if (d.index in roots.indices) current = d.index
                is Drawn.Beside -> current?.let { over.getOrPut(it) { mutableListOf() }.add(d.node) } ?: below.add(d.node)
            }
        }
        val placed = order.mapIndexed { i, idx ->
            val root = roots[idx]
            val under = if (i == 0) below else emptyList()
            root.copy(children = under + root.children + over[idx].orEmpty())
        }
        return placed + roots.indices.filter { it !in order }.map { roots[it] }
    }

    /** `window` walked in drawing order; `composeIndex` names each Compose root's View. */
    fun walk(window: View, composeIndex: Map<View, Int>): List<Drawn> {
        val ancestors = HashSet<View>()
        for (v in composeIndex.keys) {
            var p = v.parent
            while (p is View) {
                ancestors.add(p)
                p = p.parent
            }
        }
        val out = mutableListOf<Drawn>()
        fun visit(v: View) {
            if (v.visibility != View.VISIBLE) return
            composeIndex[v]?.let {
                out.add(Drawn.Root(it))
                return
            }
            if (v in ancestors && v is ViewGroup) {
                for (i in v.drawingOrder()) v.getChildAt(i)?.let(::visit)
                return
            }
            if (v is ViewRootForTest || v.id in SYSTEM_BAR_BACKGROUNDS) return
            viewSubtree(v)?.let { out.add(Drawn.Beside(it)) }
        }
        visit(window)
        return out
    }

    /// The decor's own backgrounds for the status and navigation bars. The
    /// app did not add them and they take no touch. Where the decor draws
    /// them — a window that is not edge to edge — every Compose screen would
    /// gain two empty nodes over its top and bottom edges. The fixture's
    /// emulator draws neither (edge to edge is the default there), so this
    /// is held by the rule, not by a measurement.
    private val SYSTEM_BAR_BACKGROUNDS = setOf(android.R.id.statusBarBackground, android.R.id.navigationBarBackground)

    private fun ViewGroup.drawingOrder(): List<Int> =
        TouchOrder.bottomFirst(childCount, ::getChildDrawingOrder) { getChildAt(it)?.z ?: 0f }
}
