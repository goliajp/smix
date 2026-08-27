package dev.smix.probe

import dev.smix.probe.SmixProbeProvider.Companion.isAllowedCaller
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Who the probe answers.
 *
 * It is exported — the runner is a different process and there is no
 * other way in — and what it answers with includes a password field's
 * actual characters. Both halves are asserted here: the deny half cannot
 * be shown on a device without installing a second app to be refused,
 * which is exactly why it would otherwise go unchecked.
 */
class CallerAllowlistTest {
    private val host = "com.example.app"

    @Test
    fun `adb may ask`() {
        assertTrue(isAllowedCaller("com.android.shell", host))
    }

    @Test
    fun `the smix runner may ask`() {
        assertTrue(isAllowedCaller("dev.smix.runner.test", host))
    }

    @Test
    fun `the host app may ask itself`() {
        assertTrue(isAllowedCaller(host, host))
    }

    @Test
    fun `anything else may not`() {
        assertFalse("a third-party app got in", isAllowedCaller("com.other.app", host))
    }

    @Test
    fun `a caller the system could not name may not`() {
        assertFalse("an unnamed caller got in", isAllowedCaller(null, host))
    }

    @Test
    fun `a package that merely starts with an allowed one may not`() {
        assertFalse(
            "prefix matching would let com.android.shell.evil in",
            isAllowedCaller("com.android.shell.evil", host),
        )
        assertFalse(
            "prefix matching would let the host's name be borrowed",
            isAllowedCaller("$host.evil", host),
        )
    }

    @Test
    fun `an unknown host does not turn the check off`() {
        assertFalse(
            "a null host let an arbitrary caller through",
            isAllowedCaller("com.other.app", null),
        )
        assertTrue("adb should still get in", isAllowedCaller("com.android.shell", null))
    }
}
