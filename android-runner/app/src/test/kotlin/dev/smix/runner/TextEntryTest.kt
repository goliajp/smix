package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TextEntryTest {
    @Test
    fun asciiIsTyped() {
        assertTrue(TextEntry.typable("mock@example.com 1!"))
        assertTrue(TextEntry.typable(""))
    }

    @Test
    fun anythingPastAsciiIsSet() {
        assertFalse(TextEntry.typable("東京"))
        assertFalse(TextEntry.typable("café"))
        assertFalse(TextEntry.typable("ok ☕"))
    }

    @Test
    fun theNewTextGoesAfterWhatTheFieldHeld() {
        assertEquals("東京☕カフェ", TextEntry.target("", "東京☕カフェ", masked = false))
        assertEquals("Tokyo 東京", TextEntry.target("Tokyo ", "東京", masked = false))
        assertEquals("密码", TextEntry.target("", "密码", masked = true))
    }

    @Test
    fun aMaskedFieldThatHoldsTextIsNotOverwritten() {
        assertNull(TextEntry.target("••••", "密码", masked = true))
    }

    @Test
    fun latinKeysComposedIntoHanziAreTheKeyboards() {
        // measured on a Samsung S22 in Chinese input mode
        assertEquals("probe@买票了.", TextEntry.composedByKeyboard("", "probe@买票了.", "probe@example.test"))
        assertEquals("안녕", TextEntry.composedByKeyboard("id: ", "id: 안녕", "dkssud"))
    }

    @Test
    fun aShortOrForeignAsciiFieldIsNotTheKeyboards() {
        assertNull(TextEntry.composedByKeyboard("", "probe@exa", "probe@example.test"))
        assertNull(TextEntry.composedByKeyboard("", "Probe@example.test", "probe@example.test"))
        // text past ASCII was set, not typed, so no keyboard was involved
        assertNull(TextEntry.composedByKeyboard("", "東京都", "東京"))
    }

    @Test
    fun theSentenceNamesTheKeyboardAndTheWayAroundIt() {
        val s = TextEntry.composedSentence("probe@买票了.", "com.samsung.android.honeyboard")
        assertTrue(s.contains("\"probe@买票了.\""))
        assertTrue(s.contains("(com.samsung.android.honeyboard)"))
        assertTrue(s.contains("method: setText"))
        assertTrue(s.contains("smix fill --set-text"))
        assertFalse(TextEntry.composedSentence("x", null).contains("()"))
    }
}
