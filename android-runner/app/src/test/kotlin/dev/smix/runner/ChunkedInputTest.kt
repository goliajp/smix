// Long text goes in as chunks, each read back before the next.
//
// `input text` under load drops key events, and a whole string sent in
// one shot came back one character short: 119 of 120 on the fixture,
// 2026-09-26, reproduced three times in a row at load 37 and never at
// load 6. The field has no way to say which character went missing; a
// chunk read back on its own can, because a dropped tail and a dropped
// middle look different once the chunk is short.
//
// A missing tail is typed again — only the missing part, so nothing lands
// twice. Anything else (a gap in the middle, a character nobody typed)
// is not repaired: typing more cannot put a character back in the middle
// without moving the caret, and it can only make an extra one worse.

package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Test

class ChunkedInputTest {
    @Test
    fun text_is_cut_into_chunks_of_the_given_size() {
        assertEquals(listOf("abcd", "efgh", "ij"), ChunkedInput.inputChunks("abcdefghij", 4))
    }

    @Test
    fun a_chunk_never_splits_a_surrogate_pair() {
        // U+1F600 is two UTF-16 units; the cut falls after it, not inside.
        val face = "😀"
        assertEquals(listOf("a$face", "bc"), ChunkedInput.inputChunks("a${face}bc", 2))
    }

    @Test
    fun a_chunk_that_landed_whole_is_landed() {
        assertEquals(
            ChunkedInput.ChunkOutcome.Landed,
            ChunkedInput.chunkOutcome(base = "xx", after = "xxabcd", chunk = "abcd", masked = false),
        )
    }

    @Test
    fun a_missing_tail_is_named_so_only_it_is_typed_again() {
        assertEquals(
            ChunkedInput.ChunkOutcome.MissingTail("d"),
            ChunkedInput.chunkOutcome(base = "xx", after = "xxabc", chunk = "abcd", masked = false),
        )
    }

    @Test
    fun a_missing_tail_is_found_where_the_caret_was() {
        // The caret sat between the two x's.
        assertEquals(
            ChunkedInput.ChunkOutcome.MissingTail("cd"),
            ChunkedInput.chunkOutcome(base = "xy", after = "xaby", chunk = "abcd", masked = false),
        )
    }

    @Test
    fun a_gap_in_the_middle_is_not_repaired() {
        assertEquals(
            ChunkedInput.ChunkOutcome.Mismatch,
            ChunkedInput.chunkOutcome(base = "", after = "abd", chunk = "abcd", masked = false),
        )
    }

    @Test
    fun a_character_nobody_typed_is_not_repaired() {
        assertEquals(
            ChunkedInput.ChunkOutcome.Mismatch,
            ChunkedInput.chunkOutcome(base = "", after = "qabcd", chunk = "abcd", masked = false),
        )
    }

    @Test
    fun a_masked_field_is_judged_by_length_alone() {
        assertEquals(
            ChunkedInput.ChunkOutcome.Landed,
            ChunkedInput.chunkOutcome(base = "••", after = "••••••", chunk = "abcd", masked = true),
        )
        assertEquals(
            ChunkedInput.ChunkOutcome.MissingTail("cd"),
            ChunkedInput.chunkOutcome(base = "••", after = "••••", chunk = "abcd", masked = true),
        )
        assertEquals(
            ChunkedInput.ChunkOutcome.Mismatch,
            ChunkedInput.chunkOutcome(base = "••", after = "•••••••", chunk = "abcd", masked = true),
        )
    }
}

/// The whole loop, against a field that drops what it is told to.
///
/// `drops` maps the n-th `input text` call (0-based) to what that call
/// loses: the field applies the typed string with those characters
/// removed. No device and no load — the readback sequences a busy
/// emulator produces, written down.
class ChunkedLoopTest {
    private class Field(var held: String, val drops: Map<Int, (String) -> String>) {
        var calls = 0
        fun type(sent: String): String {
            val landed = drops[calls]?.invoke(sent) ?: sent
            calls++
            held += landed
            return held
        }
    }

    /// `callsInTime` is how many `input text` calls fit the budget; null is no budget.
    private fun run(
        text: String,
        drops: Map<Int, (String) -> String>,
        before: String = "",
        callsInTime: Int? = null,
    ) = Field(before, drops).let { f ->
        val outOfTime = { callsInTime != null && f.calls >= callsInTime }
        f to ChunkedInput.typeInChunks(text, before, masked = false, chunkPoints = 4, retypes = 3, outOfTime) { sent, _, _ ->
            ChunkedInput.Reading(f.type(sent), present = true, focused = true)
        }
    }

    @Test
    fun a_spent_budget_stops_before_the_next_chunk_and_says_what_landed() {
        val (f, r) = run("abcdefghij", emptyMap(), callsInTime = 2)
        assertEquals(ChunkedInput.ChunkedResult.OutOfTime(held = "abcdefgh", chunk = 2, of = 3), r)
        assertEquals(2, f.calls)
    }

    @Test
    fun a_spent_budget_stops_before_a_retype_and_reports_the_short_chunk() {
        // The second call loses its tail; the retype would be the third call.
        val (f, r) = run("abcdefgh", mapOf(1 to { s: String -> s.dropLast(2) }), callsInTime = 2)
        assertEquals(ChunkedInput.ChunkedResult.OutOfTime(held = "abcdef", chunk = 1, of = 2), r)
        assertEquals(2, f.calls)
    }

    @Test
    fun a_budget_spent_before_anything_was_typed_types_nothing() {
        val (f, r) = run("abcd", emptyMap(), before = "x", callsInTime = 0)
        assertEquals(ChunkedInput.ChunkedResult.OutOfTime(held = "x", chunk = 0, of = 1), r)
        assertEquals(0, f.calls)
    }

    @Test
    fun a_text_typed_cleanly_takes_one_call_per_chunk() {
        val (f, r) = run("abcdefghij", emptyMap())
        assertEquals(ChunkedInput.ChunkedResult.Done("abcdefghij", chunks = 3, retyped = 0), r)
        assertEquals(3, f.calls)
    }

    @Test
    fun a_dropped_tail_is_typed_again_and_nothing_lands_twice() {
        // The second call ("efgh") loses its last two characters.
        val (f, r) = run("abcdefghij", mapOf(1 to { s: String -> s.dropLast(2) }))
        assertEquals(ChunkedInput.ChunkedResult.Done("abcdefghij", chunks = 3, retyped = 1), r)
        assertEquals(4, f.calls)
    }

    @Test
    fun a_chunk_that_did_not_arrive_at_all_is_typed_again_whole() {
        val (_, r) = run("abcdefgh", mapOf(0 to { _: String -> "" }))
        assertEquals(ChunkedInput.ChunkedResult.Done("abcdefgh", chunks = 2, retyped = 1), r)
    }

    @Test
    fun a_dropped_first_character_fails_without_typing_more() {
        val (f, r) = run("abcdefgh", mapOf(1 to { s: String -> s.drop(1) }))
        assertEquals(
            ChunkedInput.ChunkedResult.Failed(held = "abcdfgh", chunk = 1, of = 2, retries = 0),
            r,
        )
        assertEquals(2, f.calls)
    }

    @Test
    fun a_dropped_middle_character_fails_without_typing_more() {
        val (f, r) = run("abcdefgh", mapOf(0 to { s: String -> s.removeRange(1, 2) }))
        assertEquals(ChunkedInput.ChunkedResult.Failed(held = "acd", chunk = 0, of = 2, retries = 0), r)
        assertEquals(1, f.calls)
    }

    @Test
    fun a_tail_that_keeps_dropping_stops_at_the_retype_limit() {
        val alwaysShort = (0..10).associateWith { { s: String -> s.dropLast(1) } }
        val (f, r) = run("abcd", alwaysShort)
        assertEquals(ChunkedInput.ChunkedResult.Failed(held = "abc", chunk = 0, of = 1, retries = 3), r)
        assertEquals(4, f.calls)
    }

    /// A field read through `reads`, one reading per `input text` call,
    /// the last repeated once they run out; the calls are counted so a
    /// test can say nothing more was typed.
    private fun runReading(text: String, reads: List<ChunkedInput.Reading>): Pair<Int, ChunkedInput.ChunkedResult> {
        var calls = 0
        val r = ChunkedInput.typeInChunks(text, "", masked = false, chunkPoints = 4, retypes = 3, { false }) { _, _, _ ->
            reads.getOrElse(calls++) { reads.last() }
        }
        return calls to r
    }

    @Test
    fun a_field_that_leaves_with_everything_sent_is_not_typed_into_again() {
        // The app submits the full field and takes it off the screen.
        val (calls, r) = runReading(
            "abcd",
            listOf(ChunkedInput.Reading("", present = false, focused = false)),
        )
        assertEquals(ChunkedInput.ChunkedResult.FieldLeft(held = "", chunk = 0, of = 1, allSent = true), r)
        assertEquals(1, calls)
    }

    @Test
    fun a_field_that_leaves_with_text_still_to_type_stops_there() {
        val (calls, r) = runReading(
            "abcdefgh",
            listOf(
                ChunkedInput.Reading("abcd", present = true, focused = true),
                ChunkedInput.Reading("", present = false, focused = false),
            ),
        )
        assertEquals(ChunkedInput.ChunkedResult.FieldLeft(held = "abcd", chunk = 1, of = 2, allSent = true), r)
        assertEquals(2, calls)
    }

    @Test
    fun a_field_that_leaves_before_the_last_chunk_says_not_everything_was_sent() {
        val (calls, r) = runReading(
            "abcdefghij",
            listOf(
                ChunkedInput.Reading("abcd", present = true, focused = true),
                ChunkedInput.Reading("", present = false, focused = false),
            ),
        )
        assertEquals(ChunkedInput.ChunkedResult.FieldLeft(held = "abcd", chunk = 1, of = 3, allSent = false), r)
        assertEquals(2, calls)
    }

    @Test
    fun a_short_chunk_in_a_field_that_lost_focus_is_not_typed_again() {
        // A retype goes wherever focus is now, which is not this field.
        val (calls, r) = runReading(
            "abcd",
            listOf(ChunkedInput.Reading("ab", present = true, focused = false)),
        )
        assertEquals(ChunkedInput.ChunkedResult.FocusLeft(held = "ab", chunk = 0, of = 1), r)
        assertEquals(1, calls)
    }

    @Test
    fun a_whole_chunk_in_a_field_that_lost_focus_has_landed() {
        // A field that submits itself when full drops focus as the last
        // character lands; what it holds is still the answer.
        val (_, r) = runReading(
            "abcd",
            listOf(ChunkedInput.Reading("abcd", present = true, focused = false)),
        )
        assertEquals(ChunkedInput.ChunkedResult.Done("abcd", chunks = 1, retyped = 0), r)
    }
}
