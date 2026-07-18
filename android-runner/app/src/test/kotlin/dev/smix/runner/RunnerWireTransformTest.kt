// Pure transform coverage: norm→pixel projection, swipe-once geometry
// (maestro "see direction" semantics), long-press step calc, `input
// text` escaping, shell command assembly, KeyName → KEYCODE mapping.

package dev.smix.runner

import android.view.KeyEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class RunnerWireTransformTest {

    // MARK: - normToPixel

    @Test
    fun normToPixelProjectsAndClamps() {
        assertEquals(540, RunnerWire.normToPixel(0.5, 1080))
        assertEquals(0, RunnerWire.normToPixel(0.0, 1080))
        // 1.0 lands exactly on the extent → clamped inside the display.
        assertEquals(1079, RunnerWire.normToPixel(1.0, 1080))
        assertEquals(1079, RunnerWire.normToPixel(2.5, 1080))
        assertEquals(0, RunnerWire.normToPixel(-0.5, 1080))
    }

    // MARK: - swipeOnceCoords (maestro convention: direction = what to SEE)

    @Test
    fun swipeDownGesturesUp() {
        val q = RunnerWire.swipeOnceCoords("down", 1000, 2000)!!
        assertEquals(RunnerWire.SwipeQuad(500, 1400, 500, 600), q)
    }

    @Test
    fun swipeUpGesturesDown() {
        val q = RunnerWire.swipeOnceCoords("up", 1000, 2000)!!
        assertEquals(RunnerWire.SwipeQuad(500, 600, 500, 1400), q)
    }

    @Test
    fun swipeLeftGesturesRight() {
        val q = RunnerWire.swipeOnceCoords("left", 1000, 2000)!!
        assertEquals(RunnerWire.SwipeQuad(300, 1000, 700, 1000), q)
    }

    @Test
    fun swipeRightGesturesLeft() {
        val q = RunnerWire.swipeOnceCoords("right", 1000, 2000)!!
        assertEquals(RunnerWire.SwipeQuad(700, 1000, 300, 1000), q)
    }

    @Test
    fun swipeUnknownDirectionReturnsNull() {
        assertNull(RunnerWire.swipeOnceCoords("diagonal", 1000, 2000))
    }

    // MARK: - longPressSteps

    @Test
    fun longPressStepsAre5msEachWithFloorOfOne() {
        assertEquals(100, RunnerWire.longPressSteps(500L))
        assertEquals(1, RunnerWire.longPressSteps(0L))
        assertEquals(1, RunnerWire.longPressSteps(4L))
        assertEquals(240, RunnerWire.longPressSteps(1200L))
    }

    // MARK: - input text escaping

    @Test
    fun escapeReplacesSpacesWithPercentS() {
        assertEquals("hello%sworld", RunnerWire.escapeForInputText("hello world"))
    }

    @Test
    fun escapeDoublesBackslashesBeforeSpaceRewrite() {
        assertEquals("a\\\\b", RunnerWire.escapeForInputText("a\\b"))
        assertEquals("a\\\\%sb", RunnerWire.escapeForInputText("a\\ b"))
    }

    @Test
    fun inputTextCommandAssembles() {
        assertEquals("input text hello%sworld", RunnerWire.inputTextCommand("hello world"))
    }

    // MARK: - foreground command

    @Test
    fun foregroundCommandTargetsMainActivitySingleTop() {
        assertEquals(
            "am start --activity-single-top -n com.example.app/.MainActivity",
            RunnerWire.foregroundCommand("com.example.app"),
        )
    }

    // MARK: - KeyMap (literals cross-checked against the framework constants)

    @Test
    fun keyMapMatchesKeyEventConstants() {
        assertEquals(KeyEvent.KEYCODE_ENTER, KeyMap.androidKeyCode("return"))
        assertEquals(KeyEvent.KEYCODE_DEL, KeyMap.androidKeyCode("delete"))
        assertEquals(KeyEvent.KEYCODE_TAB, KeyMap.androidKeyCode("tab"))
        assertEquals(KeyEvent.KEYCODE_SPACE, KeyMap.androidKeyCode("space"))
        assertEquals(KeyEvent.KEYCODE_ESCAPE, KeyMap.androidKeyCode("escape"))
        assertEquals(KeyEvent.KEYCODE_DPAD_UP, KeyMap.androidKeyCode("arrowUp"))
        assertEquals(KeyEvent.KEYCODE_DPAD_DOWN, KeyMap.androidKeyCode("arrowDown"))
        assertEquals(KeyEvent.KEYCODE_DPAD_LEFT, KeyMap.androidKeyCode("arrowLeft"))
        assertEquals(KeyEvent.KEYCODE_HOME, KeyMap.androidKeyCode("home"))
        assertEquals(KeyEvent.KEYCODE_POWER, KeyMap.androidKeyCode("lock"))
        assertEquals(KeyEvent.KEYCODE_VOLUME_UP, KeyMap.androidKeyCode("volumeUp"))
        assertEquals(KeyEvent.KEYCODE_VOLUME_DOWN, KeyMap.androidKeyCode("volumeDown"))
        assertEquals(KeyEvent.KEYCODE_DPAD_RIGHT, KeyMap.androidKeyCode("arrowRight"))
    }

    @Test
    fun keyMapUnknownNameReturnsNull() {
        assertNull(KeyMap.androidKeyCode("fnord"))
    }
}
