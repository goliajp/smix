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

    /// The keyboard in handwriting mode, measured on API 36 after a stylus
    /// stroke: it takes touches on a toolbar by the field and a strip at the
    /// bottom, and its bounds are the rectangle around both. A real touch at
    /// (878, 1455), between the pieces, opened the app's code screen.
    private val handwriting = HitChain.Window(
        layer = 1,
        bounds = HitChain.Box(0, 847, 1080, 2340),
        root = HitChain.Node(
            id = "", label = "", bounds = HitChain.Box(0, 847, 1080, 2340),
            children = listOf(
                HitChain.Node(
                    id = "toolbar", label = "", bounds = HitChain.Box(22, 847, 176, 1579), children = emptyList(),
                ),
            ),
        ),
        touch = listOf(HitChain.Box(22, 847, 176, 1579), HitChain.Box(0, 2274, 1080, 2340)),
    )

    private val withCode = activity.copy(
        root = activity.root.copy(
            children = listOf(
                HitChain.Node(
                    id = "fixture_open_code", label = "",
                    bounds = HitChain.Box(718, 1389, 1039, 1521), children = emptyList(),
                ),
            ),
        ),
    )

    @Test
    fun aPointBetweenTheKeyboardsPiecesIsTheApps() {
        val chain = HitChain.at(listOf(withCode, handwriting), 878, 1455)
        assertEquals("the touch goes to the app: $chain", "fixture_open_code", chain.firstOrNull()?.id)
    }

    @Test
    fun aPointOnTheKeyboardsToolbarIsTheKeyboards() {
        val chain = HitChain.at(listOf(withCode, handwriting), 100, 1000)
        assertEquals("the touch goes to the toolbar: $chain", "toolbar", chain.firstOrNull()?.id)
    }

    @Test
    fun aWindowWhoseTouchesWereNotReadTakesThemOverItsBounds() {
        val chain = HitChain.at(listOf(withCode, handwriting.copy(touch = null)), 878, 1455)
        assertTrue("the keyboard's bounds hold the point, so its window answers: $chain", chain.none { it.id == "fixture_open_code" })
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

    // A consumer's player shell draws its controls (child 0) over a
    // full-size layer added after them (child 1). Touches go to the
    // controls; the chain went down the layer, and every tap on a control
    // was reported as a miss.
    @Test
    fun the_child_drawn_on_top_takes_the_point_whatever_its_index() {
        val shell = HitChain.Window(
            layer = 0,
            bounds = HitChain.Box(0, 0, 1080, 2340),
            root = HitChain.Node(
                "player_shell", "", HitChain.Box(0, 0, 1080, 800),
                listOf(
                    HitChain.Node("btn_player_fullscreen", "", HitChain.Box(900, 650, 1060, 790), emptyList(), drawingOrder = 2),
                    HitChain.Node("", "gesture layer", HitChain.Box(0, 0, 1080, 800), emptyList(), drawingOrder = 1),
                ),
            ),
        )
        assertEquals("btn_player_fullscreen", HitChain.at(listOf(shell), 980, 720).first().id)
    }

    @Test
    fun siblings_drawn_in_index_order_are_read_as_before() {
        val group = HitChain.Window(
            layer = 0,
            bounds = HitChain.Box(0, 0, 1080, 2340),
            root = HitChain.Node(
                "group", "", HitChain.Box(0, 0, 1080, 800),
                listOf(
                    HitChain.Node("below", "", HitChain.Box(0, 0, 1080, 800), emptyList(), drawingOrder = 1),
                    HitChain.Node("above", "", HitChain.Box(0, 0, 1080, 800), emptyList(), drawingOrder = 2),
                ),
            ),
        )
        assertEquals("above", HitChain.at(listOf(group), 10, 10).first().id)
    }
}
