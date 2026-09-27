// Typing long text in chunks, each read back before the next.
//
// Pure given the `send` it is handed, so every readback sequence a busy
// device produces — a dropped tail, a field that submits itself and
// leaves — can be tested without one.

package dev.smix.runner

import org.json.JSONObject

object ChunkedInput {
    /// What one chunk of text did to the field it was typed into.
    sealed class ChunkOutcome {
        object Landed : ChunkOutcome()
        data class MissingTail(val rest: String) : ChunkOutcome()
        object Mismatch : ChunkOutcome()
    }

    /// One read of the field that was typed into.
    ///
    /// `present` is whether the node is still on screen at all: a field
    /// that submits itself when full can take the screen away with it,
    /// and a node that has gone reads as empty, which is not what it
    /// held. `focused` is whether typing more would still reach it —
    /// `input text` goes wherever focus is, not to a node.
    data class Reading(val text: String, val present: Boolean, val focused: Boolean)

    /// How a chunked `input text` ended.
    ///
    /// `send` types `sent` and reads the field afterwards; `base` and
    /// `chunk` are passed so a caller can wait for the chunk to land
    /// before reading.
    sealed class ChunkedResult {
        data class Done(val held: String, val chunks: Int, val retyped: Int) : ChunkedResult()
        data class Failed(val held: String, val chunk: Int, val of: Int, val retries: Int) : ChunkedResult()

        /// The caller's budget ran out before chunk `chunk` was typed.
        data class OutOfTime(val held: String, val chunk: Int, val of: Int) : ChunkedResult()

        /// The field left the screen after chunk `chunk` was sent. `held`
        /// is its last reading while it was there. `allSent` says whether
        /// that was the last chunk: an app that submits a full field and
        /// moves on has been given everything, and nothing is left to type.
        data class FieldLeft(val held: String, val chunk: Int, val of: Int, val allSent: Boolean) :
            ChunkedResult()

        /// Chunk `chunk` came up short and the field no longer has focus,
        /// so typing its tail again would put it in whatever does.
        data class FocusLeft(val held: String, val chunk: Int, val of: Int) : ChunkedResult()
    }

    fun typeInChunks(
        text: String,
        before: String,
        masked: Boolean,
        chunkPoints: Int,
        retypes: Int,
        outOfTime: () -> Boolean,
        send: (sent: String, base: String, chunk: String) -> Reading,
    ): ChunkedResult {
        val chunks = inputChunks(text, chunkPoints)
        var held = before
        var seen = before
        var retyped = 0
        for ((index, chunk) in chunks.withIndex()) {
            val base = held
            var sent = chunk
            var tries = 0
            while (true) {
                // Checked before every `input text`, a retype included: one
                // already sent cannot be taken back, and an answer after the
                // host stopped waiting reaches nobody.
                if (outOfTime()) return ChunkedResult.OutOfTime(seen, index, chunks.size)
                val now = send(sent, base, chunk)
                if (!now.present) {
                    return ChunkedResult.FieldLeft(seen, index, chunks.size, index == chunks.lastIndex)
                }
                seen = now.text
                when (val outcome = chunkOutcome(base, now.text, chunk, masked)) {
                    ChunkOutcome.Landed -> {
                        held = now.text
                        break
                    }
                    is ChunkOutcome.MissingTail -> {
                        if (!now.focused) return ChunkedResult.FocusLeft(now.text, index, chunks.size)
                        if (tries == retypes) return ChunkedResult.Failed(now.text, index, chunks.size, tries)
                        tries++
                        retyped++
                        sent = outcome.rest
                    }
                    ChunkOutcome.Mismatch -> return ChunkedResult.Failed(now.text, index, chunks.size, tries)
                }
            }
        }
        return ChunkedResult.Done(held, chunks.size, retyped)
    }

    /// `text` cut into pieces of at most `size` code points, never
    /// splitting a surrogate pair (a split pair is two characters nobody
    /// typed).
    fun inputChunks(text: String, size: Int): List<String> {
        val out = mutableListOf<String>()
        var start = 0
        while (start < text.length) {
            var end = start
            var points = 0
            while (end < text.length && points < size) {
                end += Character.charCount(text.codePointAt(end))
                points++
            }
            out.add(text.substring(start, end))
            start = end
        }
        return out
    }

    /// Judge one chunk: `base` is what the field held before it, `after`
    /// what it holds now.
    ///
    /// Landed when `after` is `base` with the whole chunk put in once, at
    /// one place (the caret need not be at the end). MissingTail when the
    /// chunk's first characters are there and only its end is not — the
    /// shape a dropped run of key events leaves, and the only one typing
    /// again can repair without typing a character twice. Anything else
    /// is Mismatch. A masked field answers only with its length, so it is
    /// judged by length alone and a shortfall is taken as the tail.
    fun chunkOutcome(base: String, after: String, chunk: String, masked: Boolean): ChunkOutcome {
        val grew = after.length - base.length
        if (masked) {
            return when {
                grew == chunk.length -> ChunkOutcome.Landed
                grew in 0 until chunk.length -> ChunkOutcome.MissingTail(chunk.substring(grew))
                else -> ChunkOutcome.Mismatch
            }
        }
        if (grew < 0 || grew > chunk.length) return ChunkOutcome.Mismatch
        for (at in 0..base.length) {
            if (!after.regionMatches(0, base, 0, at)) continue
            if (!after.regionMatches(at + grew, base, at, base.length - at)) continue
            if (!after.regionMatches(at, chunk, 0, grew)) continue
            return if (grew == chunk.length) {
                ChunkOutcome.Landed
            } else {
                ChunkOutcome.MissingTail(chunk.substring(grew))
            }
        }
        return ChunkOutcome.Mismatch
    }

    /// Code points per `input text` call.
    ///
    /// Measured on the fixture's field, 2026-09-26, ten tries per length
    /// with our own `yes` processes adding load: at load 11–15 and again
    /// at 19–28, 8 / 16 / 32 / 64 characters landed whole 10 of 10, and
    /// 120 landed whole 2 and 7 of 10 (the rest arrived as 101, 68 and 97
    /// characters). Half the longest length that held both times.
    const val INPUT_CHUNK_POINTS: Int = 32

    /// Times one chunk's missing tail is typed again before giving up.
    const val INPUT_CHUNK_RETYPES: Int = 3

    /// The answer for a field that left the screen once everything had
    /// been sent: the app took it and moved on, so there is nothing to
    /// read it back from. Said as that — `readBack: unread` — rather
    /// than as a field that holds nothing.
    fun leftWithEverythingBody(text: String, before: String, lastHeld: String, masked: Boolean, chunks: Int): String {
        val obj = JSONObject()
            .put("ok", true)
            .put("status", "field_left")
            .put("readBack", "unread")
            .put("text", text)
            .put("chunks", chunks)
        if (masked) {
            obj.put("beforeLength", before.length).put("lastReadLength", lastHeld.length)
        } else {
            obj.put("before", before).put("lastRead", lastHeld)
        }
        return obj.toString()
    }

    /// Why typing stopped part-way, for a field that went away or lost
    /// focus while text was still to be typed. Nothing more was sent in
    /// either case: `input text` goes where focus is, not to a node.
    fun stoppedMessage(result: ChunkedResult, text: String, whichField: String, masked: Boolean): String {
        val (held, chunk, of) = when (result) {
            is ChunkedResult.FieldLeft -> Triple(result.held, result.chunk, result.of)
            is ChunkedResult.FocusLeft -> Triple(result.held, result.chunk, result.of)
            else -> error("stoppedMessage is for a field that left or lost focus, not $result")
        }
        val had = if (masked) "${held.length} character(s)" else "\"$held\""
        val why = if (result is ChunkedResult.FieldLeft) {
            "left the screen after chunk ${chunk + 1} of $of was sent"
        } else {
            "lost focus with chunk ${chunk + 1} of $of short"
        }
        return "input-text: $whichField $why, typing \"${if (masked) "…" else text}\"; it held $had " +
            "when last read. Nothing more was typed: the rest would have gone into whatever " +
            "holds focus now, which is not this field."
    }
}
