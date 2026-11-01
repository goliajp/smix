package dev.smix.runner

import java.io.InputStream

/// A shell command the runner starts has a time limit of its own.
///
/// Routes with a budget only checked it between stages, so a command that
/// had started — a batch of delete keys, a piece of `input text` — could
/// run on for as long as it liked, and the host's request with it. A
/// command past its limit is stopped by closing its output, and named.
object ShellLimit {
    /// Longer than any one command a route sends: text goes in 32
    /// characters at a time and deletes in batches.
    const val LIMIT_MS = 10_000L

    class Overrun(val command: String, val limitMs: Long) :
        RuntimeException("the shell command `$command` was still running after $limitMs ms and was stopped")

    /// Read `output` to its end on a thread of its own and wait up to
    /// `limitMs` for that; past it, `close` the command's output and throw
    /// [Overrun].
    fun drain(command: String, output: InputStream, limitMs: Long = LIMIT_MS, close: () -> Unit) {
        read(command, output, limitMs, close)
    }

    /// [drain], keeping what the command printed.
    fun read(command: String, output: InputStream, limitMs: Long = LIMIT_MS, close: () -> Unit): String {
        val printed = java.io.ByteArrayOutputStream()
        val reader = Thread {
            val buf = ByteArray(4096)
            try {
                while (true) {
                    val n = output.read(buf)
                    if (n < 0) break
                    printed.write(buf, 0, n)
                }
            } catch (_: java.io.IOException) {
                // closed under it, below
            }
        }
        reader.isDaemon = true
        reader.start()
        reader.join(limitMs)
        if (reader.isAlive) {
            close()
            throw Overrun(command, limitMs)
        }
        return printed.toString(Charsets.UTF_8.name())
    }
}
