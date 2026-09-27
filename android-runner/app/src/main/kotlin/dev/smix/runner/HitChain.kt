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

    /// `drawingOrder` is the node's place among its siblings as drawn — and
    /// as a touch is dispatched: the highest is on top. It is not the index:
    /// a group that overrides `getChildDrawingOrder` draws in its own order.
    data class Node(
        val id: String,
        val label: String,
        val bounds: Box,
        val children: List<Node>,
        val drawingOrder: Int = 0,
    )

    /// `pkg` and `application` say whose window it is, so the probe's tree
    /// can take the place of the app's (see [withProbe]).
    data class Window(
        val layer: Int,
        val bounds: Box,
        val root: Node,
        val pkg: String? = null,
        val application: Boolean = false,
    )

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
        // One path down, into the child drawn on top of those holding the
        // point — the one a touch there is dispatched to. Equal orders keep
        // index order, the later on top: a group that sets no order of its
        // own, and the probe's nodes, which it lists as drawn.
        n.children
            .withIndex()
            .filter { it.value.bounds.contains(px, py) }
            .maxWithOrNull(compareBy({ it.value.drawingOrder }, { it.index }))
            ?.let { collect(it.value, px, py, into) }
    }

    /// A reader of the screen, named as the wire names it.
    enum class Reader(val wire: String) {
        ACCESSIBILITY("accessibility"),
        SEMANTICS("semantics"),
        ;

        companion object {
            /// The reader a caller aimed from, from its `aimedBy`. A caller
            /// that names none aimed from nothing this runner can read but
            /// the projection — a point, or an older client.
            fun named(wire: String?): Reader = entries.firstOrNull { it.wire == wire } ?: ACCESSIBILITY
        }
    }

    /// What is under the point, read from one reader — or why that reader
    /// could not be read. Never the other reader's answer in its place: the
    /// host aimed from the one it named, and a chain from another is a
    /// verdict on some other aim.
    sealed class Reading {
        data class Read(val reader: Reader, val chain: List<Entry>) : Reading()
        data class Unreadable(val reader: Reader, val why: String) : Reading()
    }

    /// The chain under `(px, py)` from `reader`. `probeRoots` is null when
    /// the app carries no probe that answered, and `app` null when there is
    /// no app to ask.
    fun read(
        reader: Reader,
        windows: List<Window>,
        px: Int,
        py: Int,
        app: String?,
        probeRoots: List<Node>?,
        display: Box,
    ): Reading = when (reader) {
        Reader.ACCESSIBILITY -> Reading.Read(reader, at(windows, px, py))
        Reader.SEMANTICS -> when {
            app == null -> Reading.Unreadable(reader, "no application window to ask a probe about")
            probeRoots == null -> Reading.Unreadable(reader, "$app's probe did not answer")
            else -> Reading.Read(reader, at(withProbe(windows, app, probeRoots, display), px, py))
        }
    }

    /// The windows with the app's own read from its semantics probe.
    ///
    /// The host aims from that tree, so the verdict on where the touch
    /// landed has to come from it too: on a Compose screen coming in, the
    /// accessibility projection lags the semantics tree by 150-300 ms
    /// (measured on the fixture), and a chain read from the projection in
    /// that gap called a tap that focused its field a miss.
    ///
    /// The same composition the host's tree is: every application window of
    /// `app` gives way to one that holds the probe's roots, stacked where the
    /// topmost of them was — or under everything else when no window of the
    /// app is listed yet — and every other window stays as the projection
    /// read it, so a keyboard or another app's dialog over the point still
    /// takes the touch.
    fun withProbe(windows: List<Window>, app: String, probeRoots: List<Node>, display: Box): List<Window> {
        val (apps, others) = windows.partition { it.application && it.pkg == app }
        val layer = apps.maxOfOrNull { it.layer } ?: ((others.minOfOrNull { it.layer } ?: 0) - 1)
        val bounds = apps.firstOrNull()?.bounds ?: display
        val root = Node("", "", bounds, probeRoots)
        return others + Window(layer, bounds, root, app, application = true)
    }

    /// The probe's roots as chain nodes: named by the test tag, or a hosted
    /// View's resource id, and its description; placed where they show. A
    /// node the layout put off screen has an empty visible rectangle, and is
    /// under no point.
    fun nodesFromProbe(roots: org.json.JSONArray): List<Node> =
        (0 until roots.length()).map { node(roots.getJSONObject(it)) }

    private fun node(o: org.json.JSONObject): Node {
        val b = (o.optJSONArray("visibleBounds") ?: o.getJSONArray("bounds"))
        val kids = o.optJSONArray("children") ?: org.json.JSONArray()
        return Node(
            id = o.optString("testTag").ifEmpty { o.optString("resourceId") },
            label = o.optString("contentDescription"),
            bounds = Box(b.getInt(0), b.getInt(1), b.getInt(2), b.getInt(3)),
            children = nodesFromProbe(kids),
        )
    }
}
