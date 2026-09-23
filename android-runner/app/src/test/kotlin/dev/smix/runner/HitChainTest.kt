package dev.smix.runner

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/// What is under a point, as the touch about to be delivered there finds it.
///
/// The geometry is the one measured on the fixture with gesture navigation:
/// a dialog window 1080x(1192..1341) over a full-screen activity, and a
/// confirm button at (773,1192 203x149). A tap aimed at that button was
/// pressed at y=2122 — outside the dialog, inside the activity — and
/// reported as a tap.
class HitChainTest {
    private val activity = HitChain.Window(
        layer = 0,
        bounds = HitChain.Box(0, 0, 1080, 2340),
        root = HitChain.Node(
            id = "", label = "", bounds = HitChain.Box(0, 0, 1080, 2340),
            children = listOf(
                HitChain.Node(
                    id = "native_dialog_count", label = "",
                    bounds = HitChain.Box(44, 357, 245, 402), children = emptyList(),
                ),
            ),
        ),
    )

    private val button1 = HitChain.Node(
        id = "button1", label = "", bounds = HitChain.Box(773, 1192, 976, 1341), children = emptyList(),
    )

    private val dialog = HitChain.Window(
        layer = 1,
        bounds = HitChain.Box(44, 900, 1036, 1396),
        root = HitChain.Node(
            id = "", label = "", bounds = HitChain.Box(44, 900, 1036, 1396),
            children = listOf(
                HitChain.Node(
                    id = "buttonPanel", label = "", bounds = HitChain.Box(44, 1150, 1036, 1396),
                    children = listOf(button1),
                ),
            ),
        ),
    )

    @Test
    fun aPointOnTheButtonHasTheButtonInnermost() {
        val chain = HitChain.at(listOf(activity, dialog), 874, 1266)
        assertEquals("innermost first, from the dialog", listOf("button1", "buttonPanel", ""), chain.map { it.id })
    }

    @Test
    fun aPointBelowTheDialogHasWhatIsBehindItAndNotTheButton() {
        val chain = HitChain.at(listOf(activity, dialog), 874, 2122)
        assertTrue("the activity's root is under y=2122: $chain", chain.isNotEmpty())
        assertTrue("button1 is not under y=2122: $chain", chain.none { it.id == "button1" })
    }

    @Test
    fun theTopmostWindowAnswersNotTheFirstListed() {
        // Listed bottom-first on purpose: the order of the window list is
        // not the order of the windows on screen, and the one on top is
        // the one the touch is delivered to.
        val chain = HitChain.at(listOf(dialog, activity), 874, 1266)
        assertEquals("the dialog is on top: $chain", "button1", chain.firstOrNull()?.id)
    }

    @Test
    fun unnamedElementsAreInTheChainWithTheirFrames() {
        // An element with neither an id nor a description is still an
        // element the touch landed on, and the host can only compare an
        // unnamed target by its frame — so the frame has to be there.
        val chain = HitChain.at(listOf(activity, dialog), 874, 2122)
        assertEquals(
            "the activity's unnamed root, with its frame: $chain",
            HitChain.Entry("", "", HitChain.Box(0, 0, 1080, 2340)),
            chain.lastOrNull(),
        )
    }

    @Test
    fun aPointInNoWindowHasNothingUnderIt() {
        assertTrue(HitChain.at(listOf(dialog), 10, 10).isEmpty())
    }
}
