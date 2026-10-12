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

    /// Whether a touch at a node stops there. `MAYBE` is a View whose own
    /// code decides each touch — an `onTouchEvent` it overrides, or a touch
    /// listener — which nothing outside the app can read.
    enum class Touch { TAKES, THROUGH, MAYBE }

    /// `drawingOrder` is the node's place among its siblings as drawn — and
    /// as a touch is dispatched: the highest is on top. It is not the index:
    /// a group that overrides `getChildDrawingOrder` draws in its own order.
    ///
    /// `touch` is `TAKES` for every node read from the accessibility tree,
    /// and for a probe too old to say: the reading these defaulted to.
    ///
    /// `clips` is whether a touch reaches the node's children only inside
    /// its bounds. A View hands a touch only to children under it; Compose
    /// hit-tests a child wherever it is placed, and a Material bottom
    /// sheet's panel sits below the container that holds it — a reader that
    /// stopped at the container's edge found only the scrim, and called a
    /// press of the sheet's button a miss.
    data class Node(
        val id: String,
        val label: String,
        val bounds: Box,
        val children: List<Node>,
        val drawingOrder: Int = 0,
        val touch: Touch = Touch.TAKES,
        val clips: Boolean = true,
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

    /// What a touch at a point finds: the elements under it, innermost
    /// first, and — when a View whose own code decides was chosen over
    /// something under it that would take the touch — why the answer is
    /// not certain.
    data class Found(val entries: List<Entry>, val uncertain: String? = null)

    /// The elements under `(px, py)` in the window a touch there is
    /// delivered to, innermost first. Empty when no window holds the point.
    fun at(windows: List<Window>, px: Int, py: Int): List<Entry> = find(windows, px, py).entries

    /// The window is the topmost one containing the point — by layer, not
    /// by its place in the list, which is not the order on screen.
    fun find(windows: List<Window>, px: Int, py: Int): Found {
        val top = windows
            .filter { it.bounds.contains(px, py) }
            .maxByOrNull { it.layer }
            ?: return Found(emptyList())
        val walk = Walk(px, py)
        walk.collect(top.root)
        return Found(walk.path.asReversed() + walk.passed, walk.uncertain)
    }

    /// One path down, into the child a touch there is dispatched to, and
    /// every layer drawn over that child which the touch went through.
    ///
    /// The child is the topmost among those holding the point that would
    /// take the touch — Compose hands it to the topmost node that acts on
    /// it, a View group to the topmost child that keeps it. A consumer's
    /// player draws its detection boxes in a full-size layer over the
    /// "GO LIVE" pill; the tap went back to live, and a chain through the
    /// boxes reported it a miss. When nothing takes it, every one of them
    /// is passed through.
    ///
    /// The layers passed through are listed because they are under the
    /// point and the touch crossed them: a label inside a card is aimed at,
    /// lets the touch through, and the card under it opens.
    private class Walk(val px: Int, val py: Int) {
        val path = mutableListOf<Entry>()
        val passed = mutableListOf<Entry>()
        var uncertain: String? = null

        fun holds(n: Node): Boolean = n.bounds.contains(px, py) || (!n.clips && n.children.any(::holds))

        fun reach(n: Node): Touch {
            if (!holds(n)) return Touch.THROUGH
            val own = if (n.bounds.contains(px, py)) n.touch else Touch.THROUGH
            return (n.children.map(::reach) + own).maxBy(::rank)
        }

        fun collect(n: Node) {
            if (n.bounds.contains(px, py)) path.add(n.entry())
            val bottomFirst = n.children.withIndex()
                .filter { holds(it.value) }
                .sortedWith(compareBy({ it.value.drawingOrder }, { it.index }))
                .map { it.value }
            val over = mutableListOf<Node>()
            var pick: Node? = null
            for (i in bottomFirst.indices.reversed()) {
                val c = bottomFirst[i]
                when (reach(c)) {
                    Touch.THROUGH -> {
                        over.add(c)
                        continue
                    }
                    Touch.TAKES -> pick = c
                    Touch.MAYBE -> {
                        pick = c
                        val under = bottomFirst.subList(0, i).lastOrNull { reach(it) != Touch.THROUGH }
                        if (under != null && uncertain == null) {
                            uncertain = "the touch reaches ${c.describe()} first, a View whose own code " +
                                "decides whether it keeps a touch; if it does not, ${under.describe()} " +
                                "under it takes it"
                        }
                    }
                }
                break
            }
            // Nothing here takes it: the touch goes through every one of them
            // to whatever above them does, which may be a gesture layer with
            // no semantics of its own — a consumer's polygon zones are boxes
            // over one, and it decides by the polygon which zone was meant.
            over.forEach(::addUnderPoint)
            pick?.let(::collect)
        }

        private fun addUnderPoint(n: Node) {
            if (n.bounds.contains(px, py)) passed.add(n.entry())
            n.children.filter(::holds).forEach(::addUnderPoint)
        }

        private fun rank(t: Touch): Int = when (t) {
            Touch.THROUGH -> 0
            Touch.MAYBE -> 1
            Touch.TAKES -> 2
        }

        private fun Node.entry() = Entry(id, label, bounds)

        private fun Node.describe(): String = when {
            id.isNotEmpty() -> "id=$id"
            label.isNotEmpty() -> "\"$label\""
            else -> "an unnamed element at (${bounds.left},${bounds.top} " +
                "${bounds.right - bounds.left}x${bounds.bottom - bounds.top})"
        }
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
        data class Read(val reader: Reader, val chain: List<Entry>, val uncertain: String? = null) : Reading()
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
        Reader.ACCESSIBILITY -> found(reader, find(windows, px, py))
        Reader.SEMANTICS -> when {
            app == null -> Reading.Unreadable(reader, "no application window to ask a probe about")
            probeRoots == null -> Reading.Unreadable(reader, "$app's probe did not answer")
            else -> found(reader, find(withProbe(windows, app, probeRoots, display), px, py))
        }
    }

    private fun found(reader: Reader, f: Found) = Reading.Read(reader, f.entries, f.uncertain)

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
        val view = o.has("className")
        return Node(
            id = o.optString("testTag").ifEmpty { o.optString("resourceId") },
            label = o.optString("contentDescription"),
            bounds = Box(b.getInt(0), b.getInt(1), b.getInt(2), b.getInt(3)),
            children = nodesFromProbe(kids),
            touch = touchOf(o, view),
            clips = view,
        )
    }

    /// The probe's own reading when it gives one; a probe older than it
    /// is read as before — a hosted View takes a touch, and a Compose node
    /// does when its semantics act on one.
    private fun touchOf(o: org.json.JSONObject, view: Boolean): Touch = when (o.optString("touch")) {
        "takes" -> Touch.TAKES
        "through" -> Touch.THROUGH
        "maybe" -> Touch.MAYBE
        else -> {
            val actions = o.optJSONArray("actions")
            val acts = actions != null && (0 until actions.length()).any { actions.optString(it) in TOUCH_ACTIONS }
            if (actions == null || view || acts) Touch.TAKES else Touch.THROUGH
        }
    }

    /// Semantics actions that mean a node acts on a touch at it, for a
    /// probe that does not say.
    private val TOUCH_ACTIONS = setOf(
        "OnClick", "OnLongClick", "ScrollBy", "ScrollByOffset", "ScrollToIndex",
        "SetProgress", "SetText", "Dismiss", "Expand", "Collapse",
    )
}
