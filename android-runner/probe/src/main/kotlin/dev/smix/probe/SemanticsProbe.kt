package dev.smix.probe

import android.os.Handler
import android.os.Looper
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

    /**
     * How long the semantics tree has looked the same, in milliseconds.
     *
     * NOT `hasPendingMeasureOrLayout`, which was the first answer here and
     * is the wrong one. Measured 2026-08-27: it read `true` throughout an
     * active fling of a lazy list — it is only false during the microseconds
     * a layout pass is actually pending, so any sample taken across a
     * process boundary lands in the gap between them. A signal that is true
     * whether or not the screen is moving cannot end a wait.
     *
     * Quiescence is the question a waiter is really asking: has anything
     * changed lately. It is computed here, in the app's process, where
     * looking is cheap, and it needs nothing internal to Compose.
     *
     * Returns -1 when nothing has been seen yet — "not settled" and "never
     * looked" are different, and one value for both is how a waiter learns
     * to trust a number that means nothing.
     */
    fun quiescentForMs(): Long {
        startSamplerOnce()
        synchronized(this) {
            return if (lastChangedAtMs == 0L) -1 else System.currentTimeMillis() - lastChangedAtMs
        }
    }

    /**
     * Watch the tree on a timer rather than only when asked.
     *
     * Sampling at call time can only answer "different from what I last
     * saw" — it cannot say WHEN it changed. Measured: after a scroll, the
     * first ask three seconds later reported zero milliseconds of quiet,
     * because the change had happened at some unknown point in between.
     *
     * Not a draw listener, which would have been cheaper: a Compose text
     * field's caret blinks, and a screen with a focused input would then
     * never look still. The semantics fingerprint ignores that — a caret
     * changes no bounds, no text and no focus.
     *
     * Started on the first ask, so an app that carries the probe and never
     * uses it pays nothing.
     */
    private fun startSamplerOnce() {
        synchronized(this) {
            if (sampler != null) return
            val h = Handler(Looper.getMainLooper())
            sampler = h
            val tick = object : Runnable {
                override fun run() {
                    sample()
                    h.postDelayed(this, SAMPLE_INTERVAL_MS)
                }
            }
            h.post(tick)
        }
    }

    private fun sample() {
        val fp = try {
            attached()
                .map { (it as RootForTest).semanticsOwner.unmergedRootSemanticsNode }
                .fold(17) { acc, n -> acc * 31 + fingerprint(n) }
        } catch (_: Exception) {
            // A root torn down mid-walk is not a change worth recording,
            // and a sampler that dies takes the signal with it silently.
            return
        }
        synchronized(this) {
            if (fp != lastFingerprint || lastChangedAtMs == 0L) {
                lastFingerprint = fp
                lastChangedAtMs = System.currentTimeMillis()
            }
        }
    }

    private var sampler: Handler? = null
    private var lastFingerprint: Int? = null
    private var lastChangedAtMs: Long = 0

    /** Fast enough to catch a fling, slow enough not to be the thing that moves. */
    private const val SAMPLE_INTERVAL_MS = 50L

    private fun fingerprint(node: SemanticsNode): Int {
        val c = node.config
        var h = node.id
        val o = node.positionOnScreen
        h = h * 31 + o.x.toInt()
        h = h * 31 + o.y.toInt()
        h = h * 31 + node.size.width
        h = h * 31 + node.size.height
        h = h * 31 + (c.getOrElseNullable(SemanticsProperties.Text) { null }?.toString()?.hashCode() ?: 0)
        h = h * 31 + (c.getOrElseNullable(SemanticsProperties.EditableText) { null }?.toString()?.hashCode() ?: 0)
        h = h * 31 + (c.getOrElseNullable(SemanticsProperties.Focused) { false }?.hashCode() ?: 0)
        for (child in node.children) h = h * 31 + fingerprint(child)
        return h
    }

    /** Every attached root's unmerged tree, as smix's wire spells it. */
    fun dumpWireJson(): String = attached()
        .map { (it as RootForTest).semanticsOwner.unmergedRootSemanticsNode.toProbeNode() }
        .toWireJson()

    /** The signal that was tried first, kept so its verdict can be re-checked. */
    fun hasPendingLayout(): Boolean = attached().any { it.hasPendingMeasureOrLayout }

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
