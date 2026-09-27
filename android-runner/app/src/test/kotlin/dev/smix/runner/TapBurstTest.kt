// A repeated tap: found once, touched `times` times, `intervalMs` apart.
//
// A consumer's QA gate opens on ten taps on a logo, each within 1.5 s of
// the last. Each Android tap read the screen and waited for its target
// first, and the gaps ran past 1.5 s.

package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Test

class TapBurstTest {
    @Test
    fun a_burst_is_read_and_a_bare_body_is_one_tap() {
        assertEquals(TapBurst(10, 200L, null), TapBurst.decode("""{"nx":0.5,"ny":0.5,"times":10,"intervalMs":200}"""))
        assertEquals(TapBurst(1, 80L, null), TapBurst.decode("""{"nx":0.5,"ny":0.5}"""))
        assertEquals(TapBurst(2, 80L, 700L), TapBurst.decode("""{"nx":0.5,"ny":0.5,"times":2,"holdMs":700}"""))
    }

    @Test(expected = IllegalArgumentException::class)
    fun zero_touches_is_refused() {
        TapBurst.decode("""{"nx":0.5,"ny":0.5,"times":0}""")
    }

    @Test
    fun ten_touches_with_the_gap_between_and_none_before() {
        val log = mutableListOf<String>()
        val ok = TapBurst(10, 200L, null).touch({ log.add("tap"); true }, { log.add("hold $it"); true }, { log.add("wait $it") })
        assertEquals(true, ok)
        assertEquals(10, log.count { it == "tap" })
        assertEquals(9, log.count { it == "wait 200" })
        assertEquals("tap", log.first())
    }

    @Test
    fun a_held_burst_holds_each_touch_and_one_that_did_not_go_in_is_said() {
        val calls = mutableListOf<Long>()
        var n = 0
        val ok = TapBurst(3, 80L, 700L).touch({ error("not held") }, { calls.add(it); ++n != 2 }, {})
        assertEquals(listOf(700L, 700L, 700L), calls)
        assertEquals(false, ok)
    }
}
