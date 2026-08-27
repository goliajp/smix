package dev.smix.probe

import androidx.compose.ui.node.RootForTest
import androidx.compose.ui.platform.ViewRootForTest
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.text.AnnotatedString
import java.util.Collections
import java.util.WeakHashMap

/**
 * The semantics tree, read in the app's own process.
 *
 * `ViewRootForTest.onViewCreatedCallback` hands over every Compose root as
 * it is created, which is why this has to be armed before the first
 * `setContent` runs — see [SmixProbeProvider], which a content provider's
 * lifetime puts ahead of `Application.onCreate`.
 *
 * Roots are held weakly. A Compose root outliving its activity would keep
 * the whole hierarchy alive, and a probe that leaks the thing it observes
 * is worse than no probe.
 */
object SemanticsProbe {
    private val roots: MutableSet<ViewRootForTest> =
        Collections.synchronizedSet(Collections.newSetFromMap(WeakHashMap()))

    private var installed = false

    /** Arm the callback. Idempotent — a second call replaces nothing. */
    @Synchronized
    fun install() {
        if (installed) return
        installed = true
        val previous = ViewRootForTest.onViewCreatedCallback
        ViewRootForTest.onViewCreatedCallback = { root ->
            roots.add(root)
            // Compose UI Test sets this too, and an app running both should
            // not have one of them silently win.
            previous?.invoke(root)
        }
    }

    /** True once no root has layout work outstanding. */
    fun isIdle(): Boolean = attached().none { it.hasPendingMeasureOrLayout }

    /** Every attached root's unmerged tree, as smix's wire spells it. */
    fun dumpWireJson(): String = attached()
        .map { (it as RootForTest).semanticsOwner.unmergedRootSemanticsNode.toProbeNode() }
        .toWireJson()

    /** Whether anything is there to read — the honest answer to "is the probe live". */
    fun rootCount(): Int = attached().size

    /**
     * Actions the probe will perform, and the ones it refuses.
     *
     * **The probe stages the screen; the touch stays real.**
     *
     * Measured on the fixture, 2026-08-27, with a dialog's scrim over
     * `compose_submit`: a real touch at that node's screen coordinates left
     * the app unchanged — correctly blocked — while semantics `OnClick`
     * returned true and the submit went through. A semantics action calls
     * the composable's own lambda and nothing in that path does hit-testing.
     *
     * Offering it as smix's tap would manufacture passes for taps a user
     * could not make, which is worse than the false-pass class the error
     * guide already names: there, an action did nothing and said it worked;
     * here, an impossible action works.
     *
     * So the surface is only what PREPARES a real touch — bringing a row
     * that was never composed into existence — after which the touch does
     * the touching. `unsafeAct` exists so the refusal above can be shown to
     * be refusing something real, and is not on the offered surface.
     */
    fun act(tag: String, action: String): String = when (action) {
        in STAGING_ACTIONS -> perform(tag, action)
        "OnClick", "PerformImeAction", "SetText" ->
            "the probe refuses `$action`: it calls the composable's lambda " +
                "without hit-testing, so it fires on a node nothing could " +
                "touch. Use smix's own tap/fill — the probe is for staging " +
                "the screen, not for acting on it."
        else -> "unknown action: $action"
    }

    /** The refused half, reachable only so a gate can show what it refuses. */
    fun unsafeAct(tag: String, action: String): String = perform(tag, action)

    /** Actions that bring a node within reach of a real touch. */
    val STAGING_ACTIONS = setOf("ScrollToIndex", "ScrollBy")

    private fun perform(tag: String, action: String): String {
        val node = attached()
            .asSequence()
            .map { (it as RootForTest).semanticsOwner.unmergedRootSemanticsNode }
            .mapNotNull { find(it, tag) }
            .firstOrNull()
            ?: return "no node carries the tag `$tag`"
        val c = node.config
        return when (action) {
            "OnClick" -> c.getOrElseNullable(SemanticsActions.OnClick) { null }
                ?.action?.invoke()
                ?.let { "OnClick returned $it" }
                ?: "this node has no OnClick action"
            else -> "unknown action: $action"
        }
    }

    private fun find(node: SemanticsNode, tag: String): SemanticsNode? {
        if (node.config.getOrElseNullable(SemanticsProperties.TestTag) { null } == tag) return node
        for (c in node.children) find(c, tag)?.let { return it }
        return null
    }

    private fun attached(): List<ViewRootForTest> =
        synchronized(roots) { roots.toList() }.filter { it.view.isAttachedToWindow }
}

internal fun SemanticsNode.toProbeNode(): ProbeNode {
    val c = config
    // Screen coordinates, not `boundsInWindow`.
    //
    // A dialog composes into its OWN window, and `boundsInWindow` is
    // relative to whichever window the node is in — so the fixture's dialog
    // reported itself at y=0 and the status bar's clock landed geometrically
    // "inside" it. Anything that compares two roots, or turns a node into a
    // tap, needs one coordinate space, and the only one shared across
    // windows is the screen's.
    val origin = positionOnScreen
    val dimensions = size
    return ProbeNode(
        id = id,
        testTag = c.getOrElseNullable(SemanticsProperties.TestTag) { null },
        // A label's text is a list because a node can carry several runs.
        text = c.getOrElseNullable(SemanticsProperties.Text) { null }
            ?.joinToString("") { it.text }
            ?.ifEmpty { null },
        // What a field actually holds. The accessibility projection cannot
        // report this on a masked field — it gives one bullet per character
        // and the characters nowhere.
        editableText = c.getOrElseNullable(SemanticsProperties.EditableText) { null }
            ?.let(AnnotatedString::toString),
        inputText = c.getOrElseNullable(SemanticsProperties.InputText) { null }
            ?.let(AnnotatedString::toString),
        contentDescription = c.getOrElseNullable(SemanticsProperties.ContentDescription) { null }
            ?.joinToString(", ")
            ?.ifEmpty { null },
        role = c.getOrElseNullable(SemanticsProperties.Role) { null }?.toString(),
        bounds = Bounds(
            origin.x.toInt(),
            origin.y.toInt(),
            origin.x.toInt() + dimensions.width,
            origin.y.toInt() + dimensions.height,
        ),
        // Compose keeps focus in its own semantics layer. Asking the
        // accessibility side instead is the wrong instrument, and reading
        // it as "nothing has focus" cost a consumer their whole suite.
        focused = c.getOrElseNullable(SemanticsProperties.Focused) { false } ?: false,
        enabled = !c.contains(SemanticsProperties.Disabled),
        // What the node will accept, rather than what a toolkit says it is.
        // `isEditable` is a claim; taking SetText is a fact.
        actions = c.mapNotNull { entry ->
            entry.key.name.takeIf { entry.value is androidx.compose.ui.semantics.AccessibilityAction<*> }
        }.sorted(),
        children = children.map { it.toProbeNode() },
    )
}
