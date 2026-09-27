package dev.smix.runner

import org.json.JSONArray
import org.json.JSONObject

/// What the touch was delivered to, in the shape the iOS runner uses,
/// plus `complete`: this chain lists every element under the point,
/// named or not, so an element's absence from it is evidence of a
/// miss. The iOS chain lists named elements only, and there absence
/// proves nothing.
///
/// `reader` says which tree the chain was read from. A reader that could
/// not be read gives no chain at all, `complete: false`, and why in
/// `readerError` — never the other reader's chain in its place.
internal fun JSONObject.withChain(landing: HitChain.Reading): JSONObject {
    val chain = when (landing) {
        is HitChain.Reading.Read -> landing.chain
        is HitChain.Reading.Unreadable ->
            return put("chain", JSONArray()).put("complete", false).put("readerError", landing.why)
    }
    val arr = JSONArray()
    for (e in chain) {
        arr.put(
            JSONObject()
                .put("identifier", e.id)
                .put("label", e.label)
                .put(
                    "frame",
                    JSONObject()
                        .put("x", e.bounds.left)
                        .put("y", e.bounds.top)
                        .put("w", e.bounds.right - e.bounds.left)
                        .put("h", e.bounds.bottom - e.bounds.top),
                ),
        )
    }
    return put("chain", arr).put("complete", true).put("reader", landing.reader.wire)
}
