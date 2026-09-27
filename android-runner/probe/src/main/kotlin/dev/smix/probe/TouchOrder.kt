package dev.smix.probe

/// A view group's children in the order they are drawn, bottom first — the
/// reverse of the order a touch looks for its target in.
///
/// Not index order. A group that overrides `getChildDrawingOrder` draws,
/// and dispatches touches, in its own order: a consumer's player shell
/// draws its controls last, over a full-size sibling added after them, and
/// a reader that took the last child by index put every tap on a control
/// into that sibling. Elevation comes first: a child raised in Z is drawn
/// over its siblings whatever its drawing position, as `ViewGroup` orders
/// them for both drawing and touch dispatch.
object TouchOrder {
    /// Child indices bottom first. `indexAtPosition` is
    /// `ViewGroup.getChildDrawingOrder`: which child is drawn at a drawing
    /// position.
    fun bottomFirst(count: Int, indexAtPosition: (Int) -> Int, z: (Int) -> Float): List<Int> =
        (0 until count).map(indexAtPosition).sortedBy(z)
}
