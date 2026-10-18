package dev.smix.runner

import java.io.InputStream

/// A request's body, read off the connection as UTF-8.
///
/// NanoHTTPD's `parseBody` decodes with the charset the Content-Type names
/// and US-ASCII when it names none, which is what a host sending
/// `application/json` sends. Every character past ASCII became U+FFFD: a
/// consumer's Wi-Fi row named `カフェ☕ Wi-Fi` was found and its tap
/// refused, because the name the runner was told to look for no longer
/// matched the one on screen, and text to type arrived as replacement
/// characters. JSON is UTF-8 whatever the header leaves out.
object RequestBody {
    /// Exactly `contentLength` bytes, or what there is before the end. A
    /// request with no length has no body: the connection is kept open, and
    /// reading on would take the next request's bytes.
    fun read(input: InputStream, contentLength: Long?): String {
        val n = (contentLength ?: 0L).toInt()
        val bytes = ByteArray(n)
        var got = 0
        while (got < n) {
            val r = input.read(bytes, got, n - got)
            if (r < 0) break
            got += r
        }
        return String(bytes, 0, got, Charsets.UTF_8)
    }
}
