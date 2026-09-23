// Has the back key landed?
//
// `UiDevice.pressBack()` returns a boolean and the boolean answers a
// third question. Its bytecode (uiautomator 2.3.0) is
// `sendKeyAndWaitForEvent(KEYCODE_BACK, 0, 2048, 1000)` — 2048 is
// `AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED`, so what it reports
// is "somebody's window content changed within a second of the key
// going in". The status bar's clock qualifies. A navigation that takes
// longer than a second, or lands without that event, does not. A
// consumer got `ok:false` twice with the screenshot taken right
// afterwards showing the screen had gone back.
//
// So the key goes in by injection (`pressKeyCode`, which is exactly
// `injectEventSync` and nothing else) and the outcome is read here,
// from successive looks at what is on screen. Same split the iOS
// runner made in `NavigationSettle`: the device half takes readings,
// the decision is a pure function that `./gradlew :app:test` can drive
// with sequences no emulator could stage on demand.

package dev.smix.runner

/**
 * One look at everything on screen.
 *
 * **Every window, not a chosen one.** The first version of this picked
 * the active window, then the active-or-focused-or-topmost one, and
 * measured on emulator-5554 the pick itself moved between two looks
 * 50ms apart: three runs of the same script gave `packageLeft`,
 * `gaveUp` and `couldNotSee` for the same back key. A reading whose
 * subject can change is not a reading of anything. Hashing the whole
 * window set removes the choice.
 *
 * [windows] is one entry per window in that list, and the entries are
 * compared one against its own id rather than folded into a single
 * number. Folding was the first version and it could not tell a window
 * that had left from a window this look failed to read: the fold
 * changed either way, so `/back` answered "the screen changed" on a
 * screen the app never left. Pairing by id is not the pick that C6
 * measured moving — what moves is which window is "the" window, not
 * whether an id still equals itself.
 *
 * A `null` in [WindowReading] means "the window is in the list and
 * this look did not read this part of it". It is not a value: it is
 * neither equal nor unequal to anything, and it is never evidence.
 *
 * Nothing here carries text. A clock, a spinner or a countdown changes
 * text every frame while nothing has gone back — the fixture's blocked
 * screen is exactly that shape, and it is what `pressBack()`'s own
 * boolean gets wrong.
 */
data class WindowReading(val id: Int, val pkg: String?, val structure: Int?)

data class ScreenReading(val windows: List<WindowReading>) {
    /** Short enough to sit in a `saw` string beside another one. */
    fun brief(): String = "windows=" + windows.joinToString(" ") {
        "${it.id}:${it.pkg ?: "<unread>"}:${it.structure ?: "<unread>"}"
    }
}

/** What one look produced. */
sealed interface Reading {
    data class Screen(val reading: ScreenReading) : Reading

    /**
     * Nothing on screen could be read.
     *
     * Not an absence: "I could not look" is not "nothing was there", so
     * it neither settles nor counts against the readings that do.
     */
    object Unreadable : Reading
}

/**
 * Whether a back key has landed, decided over successive readings.
 *
 * One instance per key press; [before] is the reading taken just before
 * the key went in.
 */
class BackSettle(private val before: ScreenReading?) {
    /**
     * How a back key can end.
     *
     * Each verdict carries the one word the wire uses and whether it
     * counts as success, so the mapping lives here rather than in a
     * `when` at the route.
     */
    enum class Verdict(val settledBy: String, val ok: Boolean) {
        /**
         * What is on screen is not what was on screen.
         *
         * Leaving the app is this too, and deliberately not a word of
         * its own: telling "went back a screen" from "went back out of
         * the app" needs a reading of which package is in front, and
         * the call that answers that (`currentPackageName`) waits for
         * the screen to go idle — measured at ten seconds a look on a
         * screen with a ticking label. A second word is not worth a
         * route that hangs on a spinner.
         */
        ArrivedScreenChanged("screenChanged", true),

        /**
         * Nothing could be read before the key went in.
         *
         * The iOS runner has a verdict shaped like this one and it
         * answers yes, because there a screen can genuinely have no
         * navigation bar to watch — "no identity" is a property of the
         * screen. Here it can only mean the look itself failed, and a
         * yes from a failed look is the false pass this route was
         * rewritten to stop making.
         */
        CouldNotSee("couldNotSee", false),

        /** The budget ran out with the readings never changing. */
        GaveUp("gaveUp", false),

        /** The key event was never injected, so nothing could follow. */
        NotInjected("notInjected", false),
    }

    private var last: ScreenReading? = null
    private var unreadable = 0
    private var compared = 0

    /** `null` means no verdict yet — look again. */
    fun observe(reading: Reading): Verdict? {
        if (before == null) return Verdict.CouldNotSee
        when (reading) {
            is Reading.Unreadable -> {
                unreadable += 1
                return null
            }
            is Reading.Screen -> {
                val now = reading.reading
                last = now
                if (now.windows.map { it.id }.toSet() != before.windows.map { it.id }.toSet()) {
                    return Verdict.ArrivedScreenChanged
                }
                val was = before.windows.associateBy { it.id }
                for (window in now.windows) {
                    val then = was[window.id] ?: continue
                    if (differs(then.structure, window.structure)) {
                        return Verdict.ArrivedScreenChanged
                    }
                    if (differs(then.pkg, window.pkg)) return Verdict.ArrivedScreenChanged
                    if (window.structure != null && then.structure != null) compared += 1
                }
                return null
            }
        }
    }

    /**
     * Whether two halves of a window's identity disagree.
     *
     * `null` is "this look did not read it", which is neither the same
     * as the other reading nor different from it. Comparing it as a
     * value is what made a missed look indistinguishable from a window
     * that had left — the whole of N1.
     */
    private fun <T> differs(then: T?, now: T?): Boolean =
        then != null && now != null && then != now

    /**
     * The verdict when the budget runs out.
     *
     * "The readings never changed" is a claim about readings, so it
     * needs one: a whole budget in which no window was ever comparable
     * is a failed look, and `gaveUp` would be that same false pass one
     * step along.
     */
    fun atDeadline(): Verdict = when {
        before == null -> Verdict.CouldNotSee
        compared == 0 -> Verdict.CouldNotSee
        else -> Verdict.GaveUp
    }

    /**
     * The readings behind the verdict.
     *
     * Exists because `gaveUp` names no branch: "the key went nowhere",
     * "the screen is genuinely identical" and "every look failed" are
     * one word otherwise.
     */
    fun saw(): String = "before=${before?.brief() ?: "<none>"} " +
        "last=${last?.brief() ?: "<none>"} unreadable=$unreadable"
}
