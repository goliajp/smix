package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.PipedInputStream
import java.io.PipedOutputStream

class ShellLimitTest {
    @Test
    fun aCommandThatEndsInTimeReturns() {
        var closed = false
        ShellLimit.drain("echo hi", ByteArrayInputStream("hi\n".toByteArray()), 1_000) { closed = true }
        assertTrue("a finished command's output was closed as an overrun", !closed)
    }

    // A command whose output never ends: the stand-in for a shell command
    // that hangs. It is stopped at its limit, and named.
    @Test
    fun aCommandStillRunningAtItsLimitIsStoppedAndNamed() {
        val sink = PipedOutputStream()
        val output = PipedInputStream(sink)
        var closed = false
        val began = System.currentTimeMillis()
        try {
            ShellLimit.drain("input text hello", output, 200) {
                closed = true
                sink.close()
            }
            fail("a command past its limit returned as if it had finished")
        } catch (e: ShellLimit.Overrun) {
            assertEquals("input text hello", e.command)
            assertTrue(e.message!!.contains("200 ms"))
        }
        assertTrue("its output was not closed", closed)
        val took = System.currentTimeMillis() - began
        assertTrue("stopped after ${took} ms, for a 200 ms limit", took in 200..2_000)
    }
}
