package dev.smix.runner

import android.app.Instrumentation
import android.graphics.Rect
import android.net.Uri
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityWindowInfo
import org.json.JSONArray

/// What a touch is about to be delivered to — see [HitChain]. Only the
/// nodes containing the point are read: the geometry is decided there, and
/// refreshing a whole tree on every tap would cost the walk `/tree` already
/// pays for.
class HitReader(private val instrumentation: Instrumentation) {
    /// The chain under `(px, py)` from `reader`, the tree the caller aimed
    /// from, and only that one; see [HitChain.read]. `app` is the package
    /// whose probe answers for the semantics tree.
    fun read(reader: HitChain.Reader, app: String?, display: HitChain.Box, px: Int, py: Int): HitChain.Reading {
        val windows = instrumentation.uiAutomation.windows.mapNotNull { w ->
            val root = w.root ?: return@mapNotNull null
            try {
                val wb = Rect()
                w.getBoundsInScreen(wb)
                HitChain.Window(
                    w.layer,
                    HitChain.Box(wb.left, wb.top, wb.right, wb.bottom),
                    node(root, px, py),
                    pkg = root.packageName?.toString(),
                    application = w.type == AccessibilityWindowInfo.TYPE_APPLICATION,
                )
            } finally {
                root.recycle()
            }
        }
        return HitChain.read(reader, windows, px, py, app, app?.let(::probeRoots), display)
    }

    /// The probe's roots for `app`, or null when it carries no probe.
    private fun probeRoots(app: String): List<HitChain.Node>? = try {
        instrumentation.context.contentResolver
            .call(Uri.parse("content://$app.smixprobe"), "tree", null, null)
            ?.getString("tree")
            ?.let { HitChain.nodesFromProbe(JSONArray(it)) }
    } catch (_: Exception) {
        // No provider under that authority, a refusal, a failure: the
        // reading says the probe did not answer, and the host is told so
        // rather than handed the projection's chain as if it were this.
        null
    }

    private fun node(n: AccessibilityNodeInfo, px: Int, py: Int): HitChain.Node {
        n.refresh()
        val r = Rect()
        n.getBoundsInScreen(r)
        val kids = mutableListOf<HitChain.Node>()
        for (i in 0 until n.childCount) {
            val c = n.getChild(i) ?: continue
            try {
                val cr = Rect()
                c.getBoundsInScreen(cr)
                if (cr.contains(px, py)) kids.add(node(c, px, py))
            } finally {
                c.recycle()
            }
        }
        return HitChain.Node(
            id = n.viewIdResourceName?.let(TreeWire::shortResourceId) ?: "",
            label = n.contentDescription?.toString() ?: "",
            bounds = HitChain.Box(r.left, r.top, r.right, r.bottom),
            children = kids,
        )
    }
}
