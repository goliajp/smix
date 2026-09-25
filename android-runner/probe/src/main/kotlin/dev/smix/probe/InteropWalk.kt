package dev.smix.probe

import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ViewRootForTest

/**
 * The Views that Compose hosts, and what the probe says about them.
 *
 * `AndroidView` puts a real View inside a composition. It has no semantics
 * node of its own unless the app wrote one, and the View under it has none
 * ever — so a probe that walks semantics children sees a hole where a
 * consumer's player chrome was, while the accessibility path sees it
 * perfectly well. Reported 2026-09-22 against
 * `R.id.btn_player_fullscreen`: `smix find` answered `exists=true` and the
 * same id in a flow answered `ELEMENT_NOT_FOUND`, because the flow reads
 * the probe's tree and `smix find` did not.
 *
 * The walk is Views all the way: the holder `AndroidView` creates sits
 * under the `AndroidComposeView`, so getting at it needs only
 * `ViewGroup.getChildAt`.
 *
 * Where the app's View starts is read from structure, not from a name.
 * No public type identifies the holder: `AndroidViewHolder` is `internal`
 * (the compiler refuses it), and `InteroperableComposeUiNode` is
 * `@InternalComposeUiApi` and not implemented by the holder anyway
 * (`javap`, ui 1.9.3). What does hold: everything the Compose root puts
 * between itself and the app's View — the handler, the holder, the layer
 * containers — is Compose's own class, in Compose's own package, and the
 * app's View is not. So the walk goes down through Compose's classes and
 * the first View that is not one is hosted content. The package is read
 * off `Modifier`, a type the compiler checks, not written as a string.
 *
 * This used to compare against `AndroidViewHolder`'s class name, which
 * any Compose release could change without a word (open-items J1). The
 * backstop stays: `two-paths-agree` drives a screen with an `AndroidView`
 * and asserts the probe sees every id the accessibility path sees, so a
 * walk that finds nothing turns it red naming the id that went missing.
 */

/** A resource name the way the accessibility path spells it. */
fun shortResourceName(full: String): String = full.substringAfter(":id/", full)

/**
 * One hosted View, or nothing when nobody has placed it.
 *
 * The two negative cases are different and must stay different:
 *
 * - **not placed** — the toolkit has not given it a position. Any
 *   rectangle would be invented, and an invented rectangle is what a
 *   consumer's `scrollUntilVisible` stopped on and then tapped: a row two
 *   days down the list, reported at a place it had never been.
 * - **placed and clipped away** — it has a position, and none of it shows.
 *   That is a fact worth carrying, with an empty rectangle, exactly as the
 *   accessibility path carries it with `visible=false`.
 */
fun interopNode(
    resourceId: String?,
    contentDescription: String?,
    text: String?,
    hint: String?,
    className: String?,
    layout: Bounds,
    clip: Bounds,
    placed: Boolean,
    focused: Boolean,
    enabled: Boolean,
    children: List<ProbeNode>,
): ProbeNode? {
    if (!placed) return null
    val shown = intersect(layout, clip)
    return ProbeNode(
        id = -1,
        testTag = null,
        resourceId = resourceId,
        text = text,
        hint = hint,
        editableText = null,
        inputText = null,
        contentDescription = contentDescription,
        role = null,
        className = className,
        bounds = layout,
        visibleBounds = shown,
        focused = focused,
        enabled = enabled,
        visible = shown.right > shown.left && shown.bottom > shown.top,
        actions = emptyList(),
        children = children,
    )
}

/**
 * Where a semantics node actually shows, in screen coordinates.
 *
 * Compose offers two numbers and they are not the same: `positionOnScreen`
 * + `size` is where the layout put the node, and `boundsInWindow` is what
 * survives every ancestor's clipping. The probe reported the first, so a
 * row scrolled out of a list came back with the rectangle it would have
 * had — measured by a consumer at `[0,533,1080,743]` with the screen
 * showing it nowhere, and `[284,1466,795,1550]` once scrolled to.
 *
 * The clipped one is in WINDOW coordinates, and a dialog composes into its
 * own window — so it is moved to the screen's before anything compares it
 * with anything else. The offset is the node's own two positions
 * subtracted, which is exactly where that window sits.
 */
fun visibleScreenRect(
    layoutOnScreen: Bounds,
    clippedInWindow: Bounds,
    windowLeftOnScreen: Int,
    windowTopOnScreen: Int,
): Bounds = intersect(
    layoutOnScreen,
    Bounds(
        clippedInWindow.left + windowLeftOnScreen,
        clippedInWindow.top + windowTopOnScreen,
        clippedInWindow.right + windowLeftOnScreen,
        clippedInWindow.bottom + windowTopOnScreen,
    ),
)

/** The part of one rectangle that lies inside the other; empty when none does. */
fun intersect(a: Bounds, b: Bounds): Bounds {
    val left = maxOf(a.left, b.left)
    val top = maxOf(a.top, b.top)
    val right = minOf(a.right, b.right)
    val bottom = minOf(a.bottom, b.bottom)
    return if (right > left && bottom > top) {
        Bounds(left, top, right, bottom)
    } else {
        // One empty rectangle rather than a negative one: downstream reads
        // width and height, and a negative width is a number that means
        // nothing while looking like a measurement.
        Bounds(left, top, left, top)
    }
}

/**
 * Every View that Compose hosts under this root, as probe nodes.
 *
 * Reported as children of the Compose ROOT rather than as roots of their
 * own. The host treats "exactly one root that every other root strictly
 * contains" as a modal and drops everything behind it — a player button
 * emitted as its own root would be read as a dialog and take the rest of
 * the screen with it. The root covers the whole Compose area and the
 * hosted View is inside it, so this containment is true as well as safe.
 */
internal fun View.hostedViews(): List<ProbeNode> {
    val found = mutableListOf<ProbeNode>()
    collectHolders(this, found)
    return found
}

/**
 * Whether a View's class is one of Compose's own, given the package
 * Compose's UI classes live in.
 *
 * A prefix with the dot, so `androidx.compose.uix` is not taken for it.
 */
fun isComposeOwned(className: String, composePackage: String): Boolean =
    className.startsWith("$composePackage.")

/** Compose UI's package, read off a type the compiler resolves. */
private fun composeUiPackage(): String = Modifier::class.java.name.substringBeforeLast('.')

private fun collectHolders(v: View, into: MutableList<ProbeNode>) {
    collectHosted(v, composeUiPackage(), into)
}

private fun collectHosted(v: View, composePackage: String, into: MutableList<ProbeNode>) {
    if (v !is ViewGroup) return
    for (i in 0 until v.childCount) {
        val child = v.getChildAt(i) ?: continue
        // A Compose root inside an interop View answers for itself —
        // it registers with the probe like any other root, and walking
        // through it here would put every node in the tree twice.
        if (child is ViewRootForTest) continue
        if (isComposeOwned(child.javaClass.name, composePackage)) {
            collectHosted(child, composePackage, into)
        } else {
            viewSubtree(child)?.let { into.add(it) }
        }
    }
}

/**
 * A whole window, walked as Views, reported as a root of its own.
 *
 * The same walk `AndroidView` content gets — not a second one. A window
 * is a root, not a child of some Compose root: it is on top of whatever
 * is behind it, and the host needs to be able to tell.
 */
internal fun View.asWindowRoot(): ProbeNode? = viewSubtree(this)

private fun viewSubtree(v: View): ProbeNode? {
    val children = mutableListOf<ProbeNode>()
    if (v is ViewGroup) {
        for (i in 0 until v.childCount) {
            val child = v.getChildAt(i) ?: continue
            if (child is ViewRootForTest) continue
            viewSubtree(child)?.let { children.add(it) }
        }
    }
    val at = IntArray(2)
    v.getLocationOnScreen(at)
    val layout = Bounds(at[0], at[1], at[0] + v.width, at[1] + v.height)
    val visible = android.graphics.Rect()
    // `getGlobalVisibleRect` answers false when none of it shows, and
    // leaves the rectangle it was handed alone — so "no" has to become an
    // empty rectangle here rather than whatever that rectangle held.
    val clip = if (v.getGlobalVisibleRect(visible)) {
        Bounds(visible.left, visible.top, visible.right, visible.bottom)
    } else {
        Bounds(layout.left, layout.top, layout.left, layout.top)
    }
    return interopNode(
        resourceId = resourceName(v),
        contentDescription = v.contentDescription?.toString()?.ifEmpty { null },
        text = (v as? TextView)?.let(::drawnText)?.ifEmpty { null },
        hint = (v as? TextView)?.hint?.toString()?.ifEmpty { null },
        className = v.javaClass.name,
        layout = layout,
        clip = clip,
        // A View in an attached hierarchy has a position; GONE gives it a
        // zero-sized one, which the clipping above already reports as
        // nothing showing.
        placed = true,
        focused = v.isFocused,
        enabled = v.isEnabled,
        children = children,
    )
}

private fun resourceName(v: View): String? {
    if (v.id == View.NO_ID) return null
    return try {
        shortResourceName(v.resources.getResourceName(v.id))
    } catch (_: android.content.res.Resources.NotFoundException) {
        // `View.generateViewId` gives an id with no resource behind it.
        // That is an ordinary state, not a fault, and the node still has
        // its description and text to be found by.
        null
    }
}

/**
 * The text a `TextView` draws, which is not always the text it holds.
 *
 * A dialog button holds "Delete" and draws "DELETE" (`textAllCaps`
 * installs a transformation). The accessibility reader reports what is
 * drawn, so a probe reporting what is held would put two spellings of
 * one button into smix's two trees — and a selector written against
 * either would find it through one reader and not the other.
 */
private fun drawnText(v: TextView): String {
    val held = v.text ?: return ""
    return v.transformationMethod?.getTransformation(held, v)?.toString() ?: held.toString()
}
