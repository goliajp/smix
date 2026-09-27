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
object StackOrder {
    /// `items` sorted by where `windowOf` puts them in `windows`; an item
    /// whose window is not listed goes to the bottom, and ties keep their
    /// order.
    fun <T> bottomFirst(items: List<T>, windowOf: (T) -> Any?, windows: List<Any>): List<T> =
        items.sortedBy { item -> windowOf(item)?.let(windows::indexOf) ?: -1 }
}
