package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AimCheckTest {
    private val pill = HitChain.Entry("btn_player_mode_go_live", "", HitChain.Box(41, 942, 212, 1068))
    private val overlay = HitChain.Entry("overlay_detections", "", HitChain.Box(0, 258, 1080, 1068))
    private val target = AimCheck.Target("btn_player_mode_go_live", "", HitChain.Box(41, 942, 212, 1068))

    private fun read(vararg chain: HitChain.Entry, uncertain: String? = null) =
        HitChain.Reading.Read(HitChain.Reader.SEMANTICS, chain.toList(), chain.toList(), uncertain)

    @Test
    fun aTargetStillUnderThePointIsTouched() {
        assertNull(AimCheck.gone(target, read(pill, overlay)))
    }

    // The consumer's C04 step 22: the pill had gone when the chain was read.
    @Test
    fun aTargetGoneFromThePointIsNotTouchedAndWhatIsThereIsNamed() {
        assertEquals("id=overlay_detections", AimCheck.gone(target, read(overlay)))
    }

    // A sheet's scrim over the target: the touch would go to the scrim, and
    // the target is still at the point under it. That is a miss for the host
    // to name, not a target that left.
    @Test
    fun aTargetCoveredAtThePointIsTouched() {
        val scrim = HitChain.Entry("", "Close panel", HitChain.Box(0, 0, 1080, 2340))
        val covered = HitChain.Reading.Read(HitChain.Reader.SEMANTICS, listOf(scrim), listOf(scrim, pill))
        assertNull(AimCheck.gone(target, covered))
    }

    @Test
    fun anEmptyPointHoldsNothing() {
        assertEquals("nothing", AimCheck.gone(target, read()))
    }

    @Test
    fun aReadingThatCannotSayLetsTheTouchGoIn() {
        assertNull(AimCheck.gone(target, read(overlay, uncertain = "a View decides")))
        assertNull(AimCheck.gone(target, HitChain.Reading.Unreadable(HitChain.Reader.SEMANTICS, "no probe")))
    }

    @Test
    fun anUnnamedTargetIsFoundByItsFrame() {
        val box = HitChain.Box(10, 10, 110, 60)
        val unnamed = AimCheck.Target("", "", box)
        assertNull(AimCheck.gone(unnamed, read(HitChain.Entry("", "", HitChain.Box(10, 11, 110, 60)))))
        assertEquals(
            "an unnamed element at (10,200)",
            AimCheck.gone(unnamed, read(HitChain.Entry("", "", HitChain.Box(10, 200, 110, 250)))),
        )
    }

    @Test
    fun theTargetIsReadFromTheRequestAndAnOlderHostSendsNone() {
        val t = AimCheck.decode(
            """{"nx":0.1,"ny":0.9,"aimedAt":{"identifier":"a","label":"A","frame":{"x":41.0,"y":942.0,"w":171.0,"h":126.0}}}""",
        )
        assertEquals(AimCheck.Target("a", "A", HitChain.Box(41, 942, 212, 1068)), t)
        assertNull(AimCheck.decode("""{"nx":0.1,"ny":0.9}"""))
    }

    // A consumer's Wi-Fi row: the name is the row's label, and it is not ASCII.
    @Test
    fun aNameThatIsNotAsciiIsTheSameName() {
        val name = "カフェ☕ Wi-Fi"
        val row = HitChain.Entry("", name, HitChain.Box(45, 1075, 1035, 1181))
        val t = AimCheck.decode(
            """{"nx":0.5,"ny":0.47,"aimedAt":{"identifier":"","label":"$name","frame":{"x":45.0,"y":1075.0,"w":990.0,"h":106.0}}}""",
        )!!
        assertNull(AimCheck.gone(t, read(row)))
    }

    // What the runner was told to look for, beside what it found: when the
    // two read the same, the difference is in what reached the runner.
    @Test
    fun aRefusalSaysWhatItLookedFor() {
        val other = HitChain.Entry("", "Lobby Guest", HitChain.Box(45, 1313, 1035, 1419))
        val t = AimCheck.Target("", "カフェ☕ Wi-Fi", HitChain.Box(45, 1075, 1035, 1181))
        val body = org.json.JSONObject(AimCheck.goneBody(AimCheck.gone(t, read(other))!!, t))
        assertEquals("target_gone_before_touch", body.getString("error"))
        assertEquals("\"Lobby Guest\"; it looked for \"カフェ☕ Wi-Fi\"", body.getString("saw"))
    }
}
