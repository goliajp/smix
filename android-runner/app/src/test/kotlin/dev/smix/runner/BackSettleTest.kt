// Has the back key landed? — decided over readings, on the JVM.
//
// The device half is only "take a reading"; this is the decision, so it
// can be driven with reading sequences instead of an emulator. Same
// split the iOS runner made for `NavigationSettle`.

package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class BackSettleTest {

    private fun screen(windows: Int = 1000, saw: String = "dev.smix.fixture") =
        ScreenReading(windows, saw)

    @Test
    fun aDifferentScreenIsArrival() {
        val settle = BackSettle(screen())
        assertEquals(
            BackSettle.Verdict.ArrivedScreenChanged,
            settle.observe(Reading.Screen(screen(windows = 2000))),
        )
    }

    @Test
    fun theSameScreenIsNotAnArrivalHoweverOftenItIsRead() {
        // The blocked fixture screen, whose label ticks five times a
        // second while the app swallows the key: nothing about what is
        // on screen changes, and the old answer was yes.
        val settle = BackSettle(screen())
        repeat(40) { assertNull(settle.observe(Reading.Screen(screen()))) }
        assertEquals(BackSettle.Verdict.GaveUp, settle.atDeadline())
    }

    @Test
    fun givingUpSaysWhatItReadBeforeAndLast() {
        val settle = BackSettle(screen(windows = 1000))
        settle.observe(Reading.Screen(screen(windows = 1000)))
        settle.atDeadline()
        val saw = settle.saw()
        assertTrue(saw, saw.contains("before="))
        assertTrue(saw, saw.contains("last="))
    }

    @Test
    fun leavingTheAppIsArrivalToo() {
        // Back at the root of an app goes to the launcher, which is a
        // different set of windows. It gets the same word as any other
        // arrival; see the verdict's own note for why it does not get
        // one of its own.
        val settle = BackSettle(screen())
        assertEquals(
            BackSettle.Verdict.ArrivedScreenChanged,
            settle.observe(Reading.Screen(screen(windows = 7, saw = "com.android.launcher3"))),
        )
    }

    @Test
    fun nothingReadableBeforeTheKeyIsAFailedLookAndNotAYes() {
        // Every Android window carries an id and a package, so "nothing
        // to compare against" can only mean the look failed — and a yes
        // from a failed look is the false pass this route exists to
        // stop making.
        val settle = BackSettle(null)
        assertEquals(
            BackSettle.Verdict.CouldNotSee,
            settle.observe(Reading.Screen(screen(windows = 2000))),
        )
        assertEquals(false, BackSettle.Verdict.CouldNotSee.ok)
        assertEquals(BackSettle.Verdict.CouldNotSee, BackSettle(null).atDeadline())
    }

    @Test
    fun anUnreadableLookIsNotEvidenceEitherWay() {
        // A gap in the evidence is not evidence: it neither settles nor
        // stops the later reading that does.
        val settle = BackSettle(screen())
        repeat(3) { assertNull(settle.observe(Reading.Unreadable)) }
        assertEquals(
            BackSettle.Verdict.ArrivedScreenChanged,
            settle.observe(Reading.Screen(screen(windows = 2000))),
        )
        assertTrue(settle.saw(), settle.saw().contains("unreadable=3"))
    }

    @Test
    fun everyVerdictSaysWhetherItIsAnOkAndCarriesOneWordForTheWire() {
        // Four, and the number is a fact about how a back key can end:
        // the screen changed, the look failed, the budget ran out, or
        // the key never went in. A fifth ending has to be added here,
        // which is the point.
        assertEquals(4, BackSettle.Verdict.values().size)
        assertEquals(
            listOf("screenChanged", "couldNotSee", "gaveUp", "notInjected"),
            BackSettle.Verdict.values().map { it.settledBy },
        )
        assertEquals(
            listOf(true, false, false, false),
            BackSettle.Verdict.values().map { it.ok },
        )
        // Every word is distinct — two endings sharing a word would be
        // two different things the host cannot tell apart.
        assertEquals(4, BackSettle.Verdict.values().map { it.settledBy }.toSet().size)
    }
}
