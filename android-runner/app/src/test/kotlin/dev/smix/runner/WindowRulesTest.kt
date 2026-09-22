// What is above the app under test, decided on the JVM.
//
// A consumer's fill failed with `no_focused_field` while their
// keyboard's own promotion dialog sat over the field. Nothing said so:
// `/system-popups` answered `[]`, and the failure answered with a
// sentence about focus. Reproduced here in the shape the emulator can
// make — `am crash` puts a dialog from package `android` over
// everything, with two buttons, and the app's own window leaves the
// stack entirely.
//
// The decision is the part that can be wrong in a way an emulator run
// would not show, so it lives on this side: window type, who owns the
// window, whether anything in it can be pressed.

package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class WindowRulesTest {

    private val app = "dev.smix.fixture"

    /// Measured on emulator-5554, 2026-09-23, with `am crash
    /// dev.smix.fixture`: package `android`, TYPE_SYSTEM, active and
    /// focused, holding "smix fixture keeps stopping" and the buttons
    /// "App info" and "Close app".
    @Test
    fun aSystemDialogOverTheAppIsAPopup() {
        assertTrue(
            "a dialog with two buttons covering the whole screen is exactly " +
                "what a caller needs told about; it was invisible because the " +
                "old rule only looked at TYPE_APPLICATION",
            WindowRules.isForeignPopup(
                type = WindowRules.TYPE_SYSTEM,
                pkg = "android",
                hasButton = true,
                takesFocus = true,
                app = app,
            ),
        )
    }

    /// The consumer's own case: the IME's promotion dialog is an
    /// ordinary application window belonging to the keyboard's package.
    @Test
    fun anotherAppsDialogIsAPopup() {
        assertTrue(
            "the window belongs to the keyboard's package and carries buttons; " +
                "the app under test cannot name any of them, so nothing but " +
                "this list can offer them",
            WindowRules.isForeignPopup(
                type = WindowRules.TYPE_APPLICATION,
                pkg = "com.adamrocker.android.input.simeji",
                hasButton = true,
                takesFocus = true,
                app = app,
            ),
        )
    }

    @Test
    fun theAppsOwnDialogIsNotAForeignWindow() {
        assertFalse(
            "an app's own dialog is answered by /tree; calling it a system " +
                "popup would send a caller to system-popup-action for a " +
                "button their own selectors can name",
            WindowRules.isForeignPopup(
                type = WindowRules.TYPE_APPLICATION,
                pkg = app,
                hasButton = true,
                takesFocus = true,
                app = app,
            ),
        )
    }

    @Test
    fun theKeyboardIsNeverAPopup() {
        for (buttons in listOf(true, false)) {
            assertFalse(
                "the input method is the keyboard, and its verb is " +
                    "hideKeyboard; listing it as a popup offers a button to " +
                    "press to make it go away and there is none",
                WindowRules.isForeignPopup(
                    type = WindowRules.TYPE_INPUT_METHOD,
                    pkg = "com.android.inputmethod.latin",
                    hasButton = buttons,
                    takesFocus = false,
                    app = app,
                ),
            )
        }
    }

    /// The two status-bar windows measured in every reading on
    /// emulator-5554: system windows owned by systemui, above the app
    /// by layer, and nothing in them to press.
    @Test
    fun aBarWithNothingToPressIsNotAPopup() {
        assertFalse(
            WindowRules.isForeignPopup(
                type = WindowRules.TYPE_SYSTEM,
                pkg = "com.android.systemui",
                hasButton = false,
                takesFocus = false,
                app = app,
            ),
        )
    }

    @Test
    fun anUnreadableOwnerIsNotGuessedAt() {
        assertFalse(
            "a window whose package could not be read cannot be said to " +
                "belong to somebody else, and saying it anyway would name " +
                "nothing in the sentence that follows",
            WindowRules.isForeignPopup(
                type = WindowRules.TYPE_APPLICATION,
                pkg = null,
                hasButton = true,
                takesFocus = true,
                app = app,
            ),
        )
    }

    @Test
    fun theSentenceNamesWhoIsOnTopAndWhatKindOfWindowItIs() {
        val said = WindowRules.windowStackSentence(
            listOf(
                WindowRules.Row(WindowRules.TYPE_SYSTEM, "com.android.systemui", 2),
                WindowRules.Row(WindowRules.TYPE_SYSTEM, "android", 0, takesFocus = true),
            ),
            app = app,
        )
        assertTrue("the owner has to be named: $said", said.contains("android"))
        assertTrue("and what kind of window it is: $said", said.contains("system"))
    }

    /// The bars are over every app at every moment. A sentence that
    /// names them in every failure is one a reader stops reading, and
    /// the failure that matters then reads exactly like the others.
    @Test
    fun theBarsAreNotNamedBecauseTheyAreAlwaysThere() {
        val said = WindowRules.windowStackSentence(
            listOf(
                WindowRules.Row(WindowRules.TYPE_SYSTEM, "com.android.systemui", 2),
                WindowRules.Row(WindowRules.TYPE_APPLICATION, app, 0, takesFocus = true),
            ),
            app = app,
        )
        assertEquals("", said)
    }

    @Test
    fun nothingOnTopSaysNothingRatherThanSomethingVague() {
        assertEquals(
            "an empty sentence is the only honest answer when the app owns " +
                "the stack; a phrase like `no foreign windows` reads the same " +
                "whether the walk happened or not",
            "",
            WindowRules.windowStackSentence(
                listOf(WindowRules.Row(WindowRules.TYPE_APPLICATION, app, 0, takesFocus = true)),
                app = app,
            ),
        )
    }

    /// The app's window leaving the stack entirely is a different
    /// story from a window over it, and the one a crash tells.
    @Test
    fun anAppWithNoWindowAtAllIsSaidEvenWhenNobodyTookFocus() {
        val said = WindowRules.windowStackSentence(
            listOf(WindowRules.Row(WindowRules.TYPE_SYSTEM, "com.android.systemui", 2)),
            app = app,
        )
        assertTrue("it has to say the app is gone: $said", said.contains("no window"))
    }

    /// Measured on emulator-5554, 2026-09-23: the navigation bar holds
    /// four clickable, named buttons — Back, Overview, Switch input
    /// method, Home. It is not a popup; it is furniture, and it never
    /// takes focus. The first version of this rule offered it as one,
    /// with `Back` among its buttons, every time the keyboard was up.
    @Test
    fun theNavigationBarHasRealButtonsAndIsStillNotAPopup() {
        assertFalse(
            "a popup is something that came up over the app and took the " +
                "focus; the bars are always there and never do",
            WindowRules.isForeignPopup(
                type = WindowRules.TYPE_SYSTEM,
                pkg = "com.android.systemui",
                hasButton = true,
                takesFocus = false,
                app = app,
            ),
        )
    }

    @Test
    fun aDialogThatTookTheFocusIsAPopupWhoeverOwnsIt() {
        assertTrue(
            "this is the whole difference between the crash dialog and the " +
                "navigation bar, and both are TYPE_SYSTEM owned by neither " +
                "the app nor each other",
            WindowRules.isForeignPopup(
                type = WindowRules.TYPE_SYSTEM,
                pkg = "android",
                hasButton = true,
                takesFocus = true,
                app = app,
            ),
        )
    }
}
