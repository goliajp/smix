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
        assertEquals(listOf("abcd", "efgh", "ij"), RunnerWire.inputChunks("abcdefghij", 4))
    }

    @Test
    fun a_chunk_never_splits_a_surrogate_pair() {
        // U+1F600 is two UTF-16 units; the cut falls after it, not inside.
        val face = "😀"
        assertEquals(listOf("a$face", "bc"), RunnerWire.inputChunks("a${face}bc", 2))
    }

    @Test
    fun a_chunk_that_landed_whole_is_landed() {
        assertEquals(
            RunnerWire.ChunkOutcome.Landed,
            RunnerWire.chunkOutcome(base = "xx", after = "xxabcd", chunk = "abcd", masked = false),
        )
    }

    @Test
    fun a_missing_tail_is_named_so_only_it_is_typed_again() {
        assertEquals(
            RunnerWire.ChunkOutcome.MissingTail("d"),
            RunnerWire.chunkOutcome(base = "xx", after = "xxabc", chunk = "abcd", masked = false),
        )
    }

    @Test
    fun a_missing_tail_is_found_where_the_caret_was() {
        // The caret sat between the two x's.
        assertEquals(
            RunnerWire.ChunkOutcome.MissingTail("cd"),
            RunnerWire.chunkOutcome(base = "xy", after = "xaby", chunk = "abcd", masked = false),
        )
    }

    @Test
    fun a_gap_in_the_middle_is_not_repaired() {
        assertEquals(
            RunnerWire.ChunkOutcome.Mismatch,
            RunnerWire.chunkOutcome(base = "", after = "abd", chunk = "abcd", masked = false),
        )
    }

    @Test
    fun a_character_nobody_typed_is_not_repaired() {
        assertEquals(
            RunnerWire.ChunkOutcome.Mismatch,
            RunnerWire.chunkOutcome(base = "", after = "qabcd", chunk = "abcd", masked = false),
        )
    }

    @Test
    fun a_masked_field_is_judged_by_length_alone() {
        assertEquals(
            RunnerWire.ChunkOutcome.Landed,
            RunnerWire.chunkOutcome(base = "••", after = "••••••", chunk = "abcd", masked = true),
        )
        assertEquals(
            RunnerWire.ChunkOutcome.MissingTail("cd"),
            RunnerWire.chunkOutcome(base = "••", after = "••••", chunk = "abcd", masked = true),
        )
        assertEquals(
            RunnerWire.ChunkOutcome.Mismatch,
            RunnerWire.chunkOutcome(base = "••", after = "•••••••", chunk = "abcd", masked = true),
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
        f to RunnerWire.typeInChunks(text, before, masked = false, chunkPoints = 4, retypes = 3, outOfTime) { sent, _, _ ->
            f.type(sent)
        }
    }

    @Test
    fun a_spent_budget_stops_before_the_next_chunk_and_says_what_landed() {
        val (f, r) = run("abcdefghij", emptyMap(), callsInTime = 2)
        assertEquals(RunnerWire.ChunkedResult.OutOfTime(held = "abcdefgh", chunk = 2, of = 3), r)
        assertEquals(2, f.calls)
    }

    @Test
    fun a_spent_budget_stops_before_a_retype_and_reports_the_short_chunk() {
        // The second call loses its tail; the retype would be the third call.
        val (f, r) = run("abcdefgh", mapOf(1 to { s: String -> s.dropLast(2) }), callsInTime = 2)
        assertEquals(RunnerWire.ChunkedResult.OutOfTime(held = "abcdef", chunk = 1, of = 2), r)
        assertEquals(2, f.calls)
    }

    @Test
    fun a_budget_spent_before_anything_was_typed_types_nothing() {
        val (f, r) = run("abcd", emptyMap(), before = "x", callsInTime = 0)
        assertEquals(RunnerWire.ChunkedResult.OutOfTime(held = "x", chunk = 0, of = 1), r)
        assertEquals(0, f.calls)
    }

    @Test
    fun a_text_typed_cleanly_takes_one_call_per_chunk() {
        val (f, r) = run("abcdefghij", emptyMap())
        assertEquals(RunnerWire.ChunkedResult.Done("abcdefghij", chunks = 3, retyped = 0), r)
        assertEquals(3, f.calls)
    }

    @Test
    fun a_dropped_tail_is_typed_again_and_nothing_lands_twice() {
        // The second call ("efgh") loses its last two characters.
        val (f, r) = run("abcdefghij", mapOf(1 to { s: String -> s.dropLast(2) }))
        assertEquals(RunnerWire.ChunkedResult.Done("abcdefghij", chunks = 3, retyped = 1), r)
        assertEquals(4, f.calls)
    }

    @Test
    fun a_chunk_that_did_not_arrive_at_all_is_typed_again_whole() {
        val (_, r) = run("abcdefgh", mapOf(0 to { _: String -> "" }))
        assertEquals(RunnerWire.ChunkedResult.Done("abcdefgh", chunks = 2, retyped = 1), r)
    }

    @Test
    fun a_dropped_first_character_fails_without_typing_more() {
        val (f, r) = run("abcdefgh", mapOf(1 to { s: String -> s.drop(1) }))
        assertEquals(
            RunnerWire.ChunkedResult.Failed(held = "abcdfgh", chunk = 1, of = 2, retries = 0),
            r,
        )
        assertEquals(2, f.calls)
    }

    @Test
    fun a_dropped_middle_character_fails_without_typing_more() {
        val (f, r) = run("abcdefgh", mapOf(0 to { s: String -> s.removeRange(1, 2) }))
        assertEquals(RunnerWire.ChunkedResult.Failed(held = "acd", chunk = 0, of = 2, retries = 0), r)
        assertEquals(1, f.calls)
    }

    @Test
    fun a_tail_that_keeps_dropping_stops_at_the_retype_limit() {
        val alwaysShort = (0..10).associateWith { { s: String -> s.dropLast(1) } }
        val (f, r) = run("abcd", alwaysShort)
        assertEquals(RunnerWire.ChunkedResult.Failed(held = "abc", chunk = 0, of = 1, retries = 3), r)
        assertEquals(4, f.calls)
    }
}
