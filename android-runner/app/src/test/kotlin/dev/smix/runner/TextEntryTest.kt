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
}
