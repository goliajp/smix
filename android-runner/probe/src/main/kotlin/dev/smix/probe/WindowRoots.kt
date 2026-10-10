package dev.smix.probe

import android.view.View
import android.view.accessibility.AccessibilityNodeInfo
import android.view.inspector.WindowInspector
import androidx.compose.ui.platform.ViewRootForTest

/**
 * This app's windows that no Compose root lives in, walked as Views.
 *
 * A Compose app that asks for confirmation through the platform's own
 * `AlertDialog` puts that dialog in a window of its own, built from
 * Views. Compose roots are the only thing this probe used to hear
 * about, so the dialog was absent from the tree a flow reads while the
 * accessibility reader listed its buttons — `smix find` said the
 * confirm button was there and `tapOn` said it was not, on the same
 * screen, measured on the fixture.
 *
 * `WindowInspector` is public API (29+, and this probe's floor is 33)
 * and lists every window root in this process. A window whose root is
 * already the root of a Compose view has been answered above.
 */
internal fun windowsWithoutCompose(compose: List<ViewRootForTest>): List<Pair<View, ProbeNode>> {
    val covered = compose.map { it.view.rootView }.toSet()
    return WindowInspector.getGlobalWindowViews()
        .filter { it.isAttachedToWindow && it.isShown && it !in covered }
        .mapNotNull { v -> v.asWindowRoot()?.let { v to it } }
}

/**
 * The accessibility window id of each window this app has on screen.
 *
 * `AccessibilityNodeInfo.getWindowId` is public, and a node made from a
 * window's root View carries that window's id. A window not yet known to
 * the accessibility service has none, and is left out rather than given one.
 *
 * This runs on a binder thread, so the node is only constructed around the
 * View, never populated from it: `createAccessibilityNodeInfo` on a
 * ViewGroup lists its children into a scratch list the window shares with
 * the main thread, and when the main thread was answering the
 * accessibility service at the same moment the app crashed with an
 * IndexOutOfBoundsException inside `ViewGroup` (once in fifteen cold
 * starts of the fixture's interop screen).
 */
internal fun accessibilityWindowIds(): IntArray =
    WindowInspector.getGlobalWindowViews()
        .filter { it.isAttachedToWindow && it.isShown }
        .map { AccessibilityNodeInfo(it).windowId }
        .filter { it != NO_WINDOW }
        .toIntArray()

/** What `getWindowId` answers for a node no window has claimed yet. */
private const val NO_WINDOW = -1
