package dev.smix.probe

import androidx.compose.ui.node.RootForTest
import androidx.compose.ui.platform.ViewRootForTest
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
