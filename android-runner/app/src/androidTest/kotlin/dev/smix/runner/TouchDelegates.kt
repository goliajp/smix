package dev.smix.runner

import android.app.UiAutomation
import android.graphics.Rect
import android.view.accessibility.AccessibilityNodeInfo

/// The touch areas a node hands to other nodes through a `TouchDelegate`,
/// on screen, by the node each one goes to, added to what its ancestors
/// handed out. The tree reports them on the node that receives them.
object TouchDelegates {
    fun handedOut(
        node: AccessibilityNodeInfo,
        bounds: Rect,
        inherited: Map<AccessibilityNodeInfo, IntArray>,
    ): Map<AccessibilityNodeInfo, IntArray> {
        val info = node.touchDelegateInfo ?: return inherited
        val out = HashMap(inherited)
        val delegating = intArrayOf(bounds.left, bounds.top, bounds.right, bounds.bottom)
        for (i in 0 until info.regionCount) {
            val region = info.getRegionAt(i) ?: continue
            val target = info.getTargetForRegion(region) ?: continue
            val r = region.bounds
            TouchArea.onScreen(intArrayOf(r.left, r.top, r.right, r.bottom), delegating)
                ?.let { out[target] = it }
        }
        return out
    }

    /// Every delegated area in the attached windows, for the probe's tree.
    fun areas(automation: UiAutomation): List<ProbeTouch.Area> {
        val out = mutableListOf<ProbeTouch.Area>()
        fun walk(node: AccessibilityNodeInfo) {
            node.touchDelegateInfo?.let { info ->
                val own = Rect().also(node::getBoundsInScreen)
                val delegating = intArrayOf(own.left, own.top, own.right, own.bottom)
                for (i in 0 until info.regionCount) {
                    val region = info.getRegionAt(i) ?: continue
                    val target = info.getTargetForRegion(region) ?: continue
                    val id = target.viewIdResourceName ?: continue
                    val r = region.bounds
                    val touch = TouchArea.onScreen(intArrayOf(r.left, r.top, r.right, r.bottom), delegating) ?: continue
                    val b = Rect().also(target::getBoundsInScreen)
                    out += ProbeTouch.Area(TreeWire.shortResourceId(id), intArrayOf(b.left, b.top, b.right, b.bottom), touch)
                }
            }
            for (i in 0 until node.childCount) node.getChild(i)?.let(::walk)
        }
        automation.windows.forEach { w -> w.root?.let(::walk) }
        return out
    }
}
