package dev.smix.runner

/// Where a `TouchDelegate` region lands on screen.
///
/// `TouchDelegateInfo` gives each region in the delegating view's own
/// coordinates. The delegating view's top left on screen moves it there,
/// and the region is cut to the delegating view's bounds: a touch outside
/// that view never reaches its delegate. Rectangles are `[left, top, right,
/// bottom]` in screen pixels; `null` when nothing of the region is left.
object TouchArea {
    fun onScreen(region: IntArray, delegating: IntArray): IntArray? {
        val left = maxOf(delegating[0] + region[0], delegating[0])
        val top = maxOf(delegating[1] + region[1], delegating[1])
        val right = minOf(delegating[0] + region[2], delegating[2])
        val bottom = minOf(delegating[1] + region[3], delegating[3])
        return if (right > left && bottom > top) intArrayOf(left, top, right, bottom) else null
    }
}
