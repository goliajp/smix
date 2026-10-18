package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Test
import java.io.ByteArrayInputStream

class RequestBodyTest {
    // JSON is UTF-8 whatever the Content-Type leaves out; the library's own
    // reading made these characters U+FFFD.
    @Test
    fun aBodyIsReadAsUtf8() {
        val json = """{"text":"東京☕カフェ"}"""
        val bytes = json.toByteArray(Charsets.UTF_8)
        assertEquals(json, RequestBody.read(ByteArrayInputStream(bytes), bytes.size.toLong()))
    }

    // The connection stays open: what follows the body is the next request.
    @Test
    fun nothingPastTheLengthIsRead() {
        val input = ByteArrayInputStream("{}GET /health HTTP/1.1".toByteArray(Charsets.UTF_8))
        assertEquals("{}", RequestBody.read(input, 2))
        assertEquals('G'.code, input.read())
    }

    @Test
    fun noLengthIsNoBody() {
        assertEquals("", RequestBody.read(ByteArrayInputStream("{}".toByteArray()), null))
    }
}
