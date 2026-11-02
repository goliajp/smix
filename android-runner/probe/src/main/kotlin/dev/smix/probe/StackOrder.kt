package dev.smix.probe

/// The probe's roots in the order their windows stand, bottom first.
///
/// A reader of the app's roots decides what a point is over by taking the
/// last root that contains it, so the order has to be the stacking one. The
/// Compose roots come out of a weak set, in no order at all: on the fixture,
/// mid-way from one activity to the next, the outgoing activity's window
/// was listed after the incoming one's and read as the one on top, while
/// the touch went to the incoming one.
///
/// `WindowInspector.getGlobalWindowViews()` lists this process's window
/// roots in the order they were added. A new activity's window and a
/// dialog's are added after what they cover, so that order is the stacking
/// one for them. It is not for an activity brought back to the front by
/// reordering an existing task, which moves its window without re-adding it.
///
/// Nor is it for a sub-window (a PopupWindow, a panel): it stands just over
/// the window it is attached to, whenever it was added. A consumer's tablet
/// card is a dialog with a full-screen scrim popup attached to the activity
/// and shown after the dialog; in add order the scrim read as the window on
/// top, and every tap inside the card was judged to land on the scrim.
object StackOrder {
    /// `items` sorted by where their windows stand; an item whose window is
    /// not listed goes to the bottom, and ties keep their order. `parentOf`
    /// names the window a sub-window is attached to, or null for a window of
    /// its own.
    fun <T> bottomFirst(
        items: List<T>,
        windowOf: (T) -> Any?,
        windows: List<Any>,
        parentOf: (Any) -> Any? = { null },
    ): List<T> {
        val stacked = stacking(windows, parentOf)
        return items.sortedBy { item -> windowOf(item)?.let(stacked::indexOf) ?: -1 }
    }

    /// `windows` bottom first: each window of its own in add order, with its
    /// sub-windows (in add order, and theirs) just above it. A sub-window
    /// whose parent is not listed stands where it was added.
    fun stacking(windows: List<Any>, parentOf: (Any) -> Any?): List<Any> {
        val attached = windows.filter { parentOf(it)?.let(windows::contains) == true }
        val out = mutableListOf<Any>()
        fun place(w: Any) {
            out += w
            attached.filter { parentOf(it) === w }.forEach(::place)
        }
        windows.filter { it !in attached }.forEach(::place)
        return out
    }
}
