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

    private val bars = WindowReading(id = 1, pkg = "com.android.systemui", structure = 500)
    private val app = WindowReading(id = 2, pkg = "dev.smix.fixture", structure = 1000)

    private fun screen(vararg windows: WindowReading = arrayOf(bars, app)) =
        ScreenReading(windows.toList())

    @Test
    fun aDifferentScreenIsArrival() {
        val settle = BackSettle(screen())
        assertEquals(
            BackSettle.Verdict.ArrivedScreenChanged,
            settle.observe(Reading.Screen(screen(bars, app.copy(structure = 2000)))),
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
    fun aWindowThatWasNotReadThisTimeIsNotAWindowThatLeft() {
        // N1, staged. The reading that flipped this route had a package
        // list holding `com.android.systemui` and nothing else, on a
        // screen the app never left — the app's window was in
        // `uiAutomation.windows` and its root came back null, and a
        // look that missed it was recorded as a look where it was gone.
        //
        // The emulator will not stage this on demand: 300 consecutive
        // looks on that screen read all three windows. It does not have
        // to. The reading sequence is what the decision sees, and here
        // it is.
        val settle = BackSettle(screen())
        repeat(40) {
            assertNull(settle.observe(Reading.Screen(screen(bars, app.copy(structure = null)))))
        }
        assertEquals(BackSettle.Verdict.GaveUp, settle.atDeadline())
    }

    @Test
    fun aWindowMissingFromTheListIsAWindowThatLeft() {
        // The other half of the distinction, and the reason it can be
        // drawn at all: `uiAutomation.windows` lists a window whose
        // root will not read, so absence from the list means absence.
        val settle = BackSettle(screen())
        assertEquals(
            BackSettle.Verdict.ArrivedScreenChanged,
            settle.observe(Reading.Screen(screen(bars))),
        )
    }

    @Test
    fun readingNoWindowAtAllForTheWholeBudgetCannotSay() {
        // "The readings never changed" is a sentence this has not
        // earned when no reading was comparable. Saying it would be the
        // same false pass one step along: a verdict from a failed look.
        val settle = BackSettle(screen())
        repeat(40) {
            assertNull(
                settle.observe(
                    Reading.Screen(
                        screen(bars.copy(structure = null), app.copy(structure = null)),
                    ),
                ),
            )
        }
        assertEquals(BackSettle.Verdict.CouldNotSee, settle.atDeadline())
    }

    @Test
    fun oneUnreadWindowDoesNotHideAnotherWindowsChange() {
        // A gap in one window is not a blindfold over the rest.
        val settle = BackSettle(screen())
        assertEquals(
            BackSettle.Verdict.ArrivedScreenChanged,
            settle.observe(
                Reading.Screen(screen(bars.copy(structure = 9), app.copy(structure = null))),
            ),
        )
    }

    @Test
    fun aPackageThatDidNotReadIsNotAChangeOfPackage() {
        // Same rule for the other half of a window's identity.
        val settle = BackSettle(screen())
        repeat(3) { assertNull(settle.observe(Reading.Screen(screen(bars, app.copy(pkg = null))))) }
        assertEquals(BackSettle.Verdict.GaveUp, settle.atDeadline())
    }

    @Test
    fun givingUpSaysWhatItReadBeforeAndLast() {
        val settle = BackSettle(screen())
        settle.observe(Reading.Screen(screen()))
        settle.atDeadline()
        val saw = settle.saw()
        assertTrue(saw, saw.contains("before="))
        assertTrue(saw, saw.contains("last="))
        // Every window in the reading reaches the line, so a reading
        // that quietly shrank is visible in the diagnosis rather than
        // only in the verdict.
        assertTrue(saw, saw.contains("com.android.systemui"))
        assertTrue(saw, saw.contains("dev.smix.fixture"))
    }

    @Test
    fun anUnreadWindowIsNamedAsUnreadInTheDiagnosis() {
        val settle = BackSettle(screen())
        settle.observe(Reading.Screen(screen(bars, app.copy(structure = null))))
        settle.atDeadline()
        assertTrue(settle.saw(), settle.saw().contains("unread"))
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
            settle.observe(
                Reading.Screen(
                    screen(bars, WindowReading(id = 9, pkg = "com.android.launcher3", structure = 7)),
                ),
            ),
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
            settle.observe(Reading.Screen(screen(bars))),
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
            settle.observe(Reading.Screen(screen(bars, app.copy(structure = 2000)))),
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
