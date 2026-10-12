package dev.smix.probe

import android.view.MotionEvent
import android.view.View
import android.widget.AbsListView
import android.widget.HorizontalScrollView
import android.widget.ScrollView
import android.widget.TextView
import androidx.compose.ui.input.pointer.PointerInputModifier
import androidx.compose.ui.layout.LayoutInfo
import androidx.compose.ui.node.ModifierNodeElement
import androidx.compose.ui.node.PointerInputModifierNode

/**
 * Whether a touch at a node stops there, as the toolkit that draws it
 * decides — the question a reader of "what is under this point" has to
 * ask of every layer it passes.
 *
 * Three answers, because one of them is honest only as a third: a View
 * that overrides `onTouchEvent`, or carries a touch listener, decides
 * each touch in code nothing outside it can read.
 *
 * - `takes` — the touch stops here (or below it): a Compose node with
 *   pointer input it does not share, a clickable View
 * - `through` — the touch goes on to whatever is drawn under it: a Compose
 *   node without such input, a View that is not clickable and leaves
 *   touches to the default handling
 * - `maybe` — a View whose own code decides
 *
 * A consumer's app showed all three being needed: a vertex handle dragged
 * with `pointerInput` and no semantics action was read as letting touches
 * through, and its zone underneath was reported as the target; a
 * full-size non-clickable View over a player's controls was read as
 * taking them.
 */
object Touch {
    const val TAKES = "takes"
    const val THROUGH = "through"
    const val MAYBE = "maybe"

    /// A Compose node takes a touch when its layout node has pointer input
    /// that keeps the touch from what is under it — `clickable`,
    /// `toggleable`, `selectable`, `draggable`, `scrollable` and
    /// `pointerInput` all do. A semantics action alone does not: it is what
    /// accessibility is offered, and a consumer's polygon zones carried
    /// `semantics { onClick }` on their bounding boxes while a gesture layer
    /// under them decided by the polygon.
    fun ofCompose(pointerInput: Boolean): String = if (pointerInput) TAKES else THROUGH

    /// `ownCode` is null when whether the View carries a touch listener
    /// could not be read.
    fun ofView(clickable: Boolean, scrolls: Boolean, ownCode: Boolean?): String = when {
        clickable || scrolls -> TAKES
        ownCode == false -> THROUGH
        else -> MAYBE
    }

    /// Whether the layout node behind a semantics node has a modifier that
    /// receives pointer input and keeps it from the siblings drawn under
    /// it. Only its own modifiers: a gesture on a descendant layout node
    /// with no semantics of its own is not visible through any public API,
    /// and reads as none.
    ///
    /// A modifier that shares its input with its siblings does not count:
    /// Compose goes on to hit-test what is drawn under it. The one that
    /// carries a touch into a hosted View is such a modifier, so a
    /// full-size `AndroidView` drawn over a player's controls — a consumer's
    /// detection overlay — lets a tap through to them; whether the View
    /// inside it keeps the touch is the View's to say.
    fun hasPointerInput(layout: LayoutInfo): Boolean = try {
        layout.getModifierInfo().any { info ->
            when (val m = info.modifier) {
                is PointerInputModifier -> !m.pointerInputFilter.shareWithSiblings
                is ModifierNodeElement<*> ->
                    (m.create() as? PointerInputModifierNode)?.sharePointerInputWithSiblings() == false
                else -> false
            }
        }
    } catch (_: LinkageError) {
        // An app on a Compose older than these types: read as a gesture,
        // so a layer is never let through on a guess.
        true
    }

    fun ofView(v: View): String = ofView(
        clickable = v.isClickable || v.isLongClickable || v.isContextClickable,
        scrolls = v is ScrollView || v is HorizontalScrollView || v is AbsListView || scrollsByName(v),
        ownCode = ownCode(v),
    )

    /// A RecyclerView, NestedScrollView or ViewPager takes the touch that
    /// might start a scroll; they are AndroidX, which this module does not
    /// depend on, so they are known by name.
    private fun scrollsByName(v: View): Boolean {
        var c: Class<*>? = v.javaClass
        while (c != null && c != View::class.java) {
            if (c.name in SCROLLING) return true
            c = c.superclass
        }
        return false
    }

    private val SCROLLING = setOf(
        "androidx.recyclerview.widget.RecyclerView",
        "androidx.core.widget.NestedScrollView",
        "androidx.viewpager.widget.ViewPager",
        "androidx.viewpager2.widget.ViewPager2",
    )

    /// Whether code of the View's own decides its touches: an `onTouchEvent`
    /// it overrides, or a touch listener. A `TextView` overrides it and
    /// keeps a touch only for links, selection or editing.
    private fun ownCode(v: View): Boolean? {
        val listener = touchListener(v) ?: return null
        if (listener) return true
        if (v is TextView) return v.movementMethod != null || v.isTextSelectable || v.onCheckIsTextEditor()
        val declared = v.javaClass.getMethod("onTouchEvent", MotionEvent::class.java).declaringClass
        return declared != View::class.java
    }

    /// Null when the listener field cannot be read on this platform.
    private fun touchListener(v: View): Boolean? = try {
        val info = View::class.java.getDeclaredField("mListenerInfo").apply { isAccessible = true }.get(v)
        if (info == null) {
            false
        } else {
            info.javaClass.getDeclaredField("mOnTouchListener").apply { isAccessible = true }.get(info) != null
        }
    } catch (_: ReflectiveOperationException) {
        null
    } catch (_: SecurityException) {
        null
    }
}
