// Which app the probe routes ask about when the caller named none.
//
// The probe is addressed by applicationId — `content://<id>.smixprobe` —
// so a request that carries no app has nothing to ask. A flow always
// names one; the CLI never did, and so `smix tree` read the
// accessibility tree while the flow standing next to it read the
// semantics tree, on the same screen, at the same moment. Two e2e
// scripts in this cycle had to curl `/probe/tree` directly because the
// CLI could not reach it.
//
// The runner is the one side that can answer without being told: it can
// see which window holds the focus. Picking is a decision, and a
// decision that only shows up on a device is a decision nothing checks
// — so it lives here, beside the window rules it reads.
//
// It is NOT the same rule as `WindowRules.isForeignPopup`. That one
// exists to report windows that do NOT belong to the app under test,
// and guessing there made a permission dialog look like the app.
// Here the guess costs nothing of the sort: asking the wrong package
// for a probe answers `present:false`, which is the same answer as
// asking nobody, and the caller falls through to the tree it always
// had.

package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ProbeTargetTest {

    private fun row(
        type: Int,
        pkg: String?,
        layer: Int,
        takesFocus: Boolean,
    ) = WindowRules.Row(type, pkg, layer, takesFocus)

    @Test
    fun theCallerIsBelievedWhenTheCallerSpoke() {
        // A flow always names its app. Nothing here may override that,
        // however the stack looks — the screen can be mid-transition,
        // and a runner that second-guessed the caller would make the
        // flow's tree depend on the animation frame it landed in.
        assertEquals(
            "a named app must be returned verbatim",
            "dev.smix.fixture",
            ProbeTarget.pick(
                listOf(row(WindowRules.TYPE_APPLICATION, "com.other.app", 10, true)),
                named = "dev.smix.fixture",
            ),
        )
    }

    @Test
    fun theFocusedApplicationWindowIsTheAppToAsk() {
        assertEquals(
            "dev.smix.fixture",
            ProbeTarget.pick(
                listOf(
                    row(WindowRules.TYPE_SYSTEM, "com.android.systemui", 20, false),
                    row(WindowRules.TYPE_APPLICATION, "dev.smix.fixture", 10, true),
                ),
                named = null,
            ),
        )
    }

    @Test
    fun anInputMethodIsNotTheAppUnderTest() {
        // The keyboard takes the focus while typing, and it is never
        // what a caller means by "the app". Measured on emulator-5554:
        // the IME window is TYPE_INPUT_METHOD and reads as focused
        // while a field is being filled.
        assertNull(
            "the keyboard was picked as the app to ask about",
            ProbeTarget.pick(
                listOf(row(WindowRules.TYPE_INPUT_METHOD, "com.google.android.inputmethod", 30, true)),
                named = null,
            ),
        )
    }

    @Test
    fun theSystemBarsAreNotTheAppUnderTest() {
        // The status and navigation bars sit above every app at all
        // times and report as active on some builds. A pick that took
        // the topmost layer would land on them.
        assertNull(
            "a system window was picked as the app to ask about",
            ProbeTarget.pick(
                listOf(
                    row(WindowRules.TYPE_SYSTEM, "com.android.systemui", 40, true),
                    row(WindowRules.TYPE_SYSTEM, "com.android.systemui", 39, true),
                ),
                named = null,
            ),
        )
    }

    @Test
    fun aStackWithNoFocusedAppWindowIsNotGuessedAt() {
        // Nothing to ask, and saying so is the answer. Inventing one
        // would send the probe a package chosen by layer order, and the
        // caller would read the result as being about their app.
        assertNull(
            ProbeTarget.pick(
                listOf(
                    row(WindowRules.TYPE_APPLICATION, "dev.smix.fixture", 10, false),
                    row(WindowRules.TYPE_SYSTEM, "com.android.systemui", 40, false),
                ),
                named = null,
            ),
        )
    }

    @Test
    fun aWindowWithNoPackageCannotBeAsked() {
        // `root.packageName` is null for a window whose root has gone
        // by the time it is read. `content://null.smixprobe` is not a
        // question.
        assertNull(
            ProbeTarget.pick(
                listOf(row(WindowRules.TYPE_APPLICATION, null, 10, true)),
                named = null,
            ),
        )
    }

    @Test
    fun theFrontmostFocusedAppWindowWins() {
        // Two application windows can hold focus flags at once during a
        // transition; the one in front is the one on the screen.
        assertEquals(
            "com.front.app",
            ProbeTarget.pick(
                listOf(
                    row(WindowRules.TYPE_APPLICATION, "com.behind.app", 5, true),
                    row(WindowRules.TYPE_APPLICATION, "com.front.app", 15, true),
                ),
                named = null,
            ),
        )
    }

    @Test
    fun anEmptyNameIsNoName() {
        // `?app=` with nothing after it, and an `App-Bundle-Id` header
        // set to the empty string, both arrive here as "". Treating
        // that as a name would address `content://.smixprobe`.
        assertEquals(
            "dev.smix.fixture",
            ProbeTarget.pick(
                listOf(row(WindowRules.TYPE_APPLICATION, "dev.smix.fixture", 10, true)),
                named = "",
            ),
        )
    }
}
