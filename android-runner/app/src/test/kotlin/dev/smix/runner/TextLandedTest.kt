// Did the characters reach the field? The answer depends on whether the
// field is willing to say what it holds.
//
// A masked field's accessibility node reports one bullet per character
// and never the characters. 6.4.0 shipped a predicate that compares the
// node's text with what was typed, which is a question that field
// cannot answer: it is false for every fill that ever worked. A
// consumer's twenty-flow Android suite stopped at the flow that signs
// in, and the same verdict was reproduced here on this repository's own
// fixture — `dispatched 10 characters and the focused field holds 10`,
// with ten bullets on screen.
//
// The masked branch is keyed on the node saying it is a password, never
// on the text looking like one. `aaaa` is four of the same character
// and is not a mask; a predicate that guessed would silently stop
// checking content for anyone whose password happens to repeat.

package dev.smix.runner

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TextLandedTest {
    @Test
    fun plaintext_landed_when_the_node_holds_what_was_typed() {
        assertTrue(
            RunnerWire.textLanded(
                before = "",
                after = "katie@example.com",
                dispatched = "katie@example.com",
                isPassword = false,
            ),
        )
    }

    @Test
    fun plaintext_short_read_is_not_landed() {
        assertFalse(
            RunnerWire.textLanded(
                before = "",
                after = "katie@example",
                dispatched = "katie@example.com",
                isPassword = false,
            ),
        )
    }

    @Test
    fun masked_landed_when_it_grew_by_what_was_dispatched() {
        assertTrue(
            RunnerWire.textLanded(
                before = "",
                after = "•".repeat(10),
                dispatched = "Sunroom!24",
                isPassword = true,
            ),
        )
    }

    @Test
    fun masked_landed_when_appended_to_what_was_already_there() {
        // The consumer's own measurement: holds 11, before 1, ten
        // dispatched. The difference was right all along.
        assertTrue(
            RunnerWire.textLanded(
                before = "•",
                after = "•".repeat(11),
                dispatched = "Sunroom!24",
                isPassword = true,
            ),
        )
    }

    @Test
    fun masked_not_landed_when_the_difference_does_not_add_up() {
        assertFalse(
            RunnerWire.textLanded(
                before = "",
                after = "•".repeat(3),
                dispatched = "Sunroom!24",
                isPassword = true,
            ),
        )
    }

    @Test
    fun a_repeated_character_in_a_plain_field_is_still_judged_by_content() {
        // Looks like a mask. Is not one. The node did not say it was.
        assertTrue(
            RunnerWire.textLanded(
                before = "",
                after = "aaaa",
                dispatched = "aaaa",
                isPassword = false,
            ),
        )
        assertFalse(
            RunnerWire.textLanded(
                before = "",
                after = "bbbb",
                dispatched = "aaaa",
                isPassword = false,
            ),
        )
    }

    @Test
    fun characters_beside_what_was_typed_are_not_landed() {
        // Measured 2026-09-25 on emulator-5554 under host load: the field
        // held `mocmock@…` for `mock@…` and this said the text had landed,
        // because the typed text was somewhere inside what the field held.
        assertFalse(
            RunnerWire.textLanded(
                before = "",
                after = "mocmock@example.test",
                dispatched = "mock@example.test",
                isPassword = false,
            ),
        )
        // And the consumer's: a `q` in front of the first text typed.
        assertFalse(
            RunnerWire.textLanded(
                before = "",
                after = "qmock@example.test",
                dispatched = "mock@example.test",
                isPassword = false,
            ),
        )
    }

    @Test
    fun text_typed_where_the_caret_was_is_landed() {
        // A tap on a field that already holds text puts the caret where
        // the tap fell, so the characters go in there, not at the end.
        assertTrue(
            RunnerWire.textLanded(
                before = "abcdef",
                after = "abcXYZdef",
                dispatched = "XYZ",
                isPassword = false,
            ),
        )
    }
}
