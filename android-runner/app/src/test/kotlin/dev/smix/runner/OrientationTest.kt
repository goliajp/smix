package dev.smix.runner

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class OrientationTest {
    private val names = listOf("portrait", "landscapeLeft", "landscapeRight", "portraitUpsideDown")

    @Test
    fun aPhoneKeepsTheRotationsMeasuredOnIt() {
        assertEquals(listOf(0, 1, 3, 2), names.map { Orientation.rotationFor(it, naturalIsLandscape = false) })
    }

    @Test
    fun aTabletCountsItsRotationsFromLandscape() {
        // portrait on a 2560x1600 tablet is a quarter turn off its
        // natural axis; rotation 0 there is landscape
        val rotations = names.map { Orientation.rotationFor(it, naturalIsLandscape = true) }
        assertEquals(listOf(3, 0, 2, 1), rotations)
        assertEquals(4, rotations.toSet().size)
    }

    @Test
    fun portraitIsNeverTheNaturalRotationOfALandscapeDisplay() {
        // the defect: "portrait" sent rotation 0 to a tablet, which is
        // landscape, and the read-back agreed with the wrong number
        val portrait = Orientation.rotationFor("portrait", naturalIsLandscape = true)!!
        assertTrue(portrait % 2 == 1)
    }

    @Test
    fun theNaturalAxisIsReadWhicheverWayTheDisplayIsTurnedNow() {
        assertTrue(Orientation.naturalIsLandscape(2560, 1600, 0))
        assertTrue(Orientation.naturalIsLandscape(1600, 2560, 3))
        assertTrue(Orientation.naturalIsLandscape(2560, 1600, 2))
        assertFalse(Orientation.naturalIsLandscape(1080, 2340, 0))
        assertFalse(Orientation.naturalIsLandscape(2340, 1080, 1))
        assertFalse(Orientation.naturalIsLandscape(1000, 1000, 0))
    }

    @Test
    fun anUnknownNameHasNoRotation() {
        assertEquals(null, Orientation.rotationFor("sideways", naturalIsLandscape = false))
        assertEquals(null, Orientation.rotationFor("landscape", naturalIsLandscape = true))
    }

    @Test
    fun onlyTheRotationUiAutomatorCannotFreezeGoesThroughSettings() {
        assertEquals(
            listOf(Orientation.Turn.NATURAL, Orientation.Turn.LEFT, Orientation.Turn.SETTINGS, Orientation.Turn.RIGHT),
            (0..3).map { Orientation.turnFor(it) },
        )
        assertEquals("settings put system user_rotation 2", Orientation.settingsCommands(2).last())
    }

    @Test
    fun theBodySaysWhetherTheRotationTook() {
        val ok = JSONObject(RunnerWire.setOrientationBody(true, "landscapeRight", 3))
        assertEquals("ok", ok.getString("status"))
        assertEquals("landscapeRight", ok.getString("orientation"))
        val not = JSONObject(RunnerWire.setOrientationBody(false, "portrait", 0))
        assertEquals(false, not.getBoolean("ok"))
        assertEquals("rotation_did_not_take", not.getString("status"))
    }

    @Test
    fun aRotationAnAppPinnedSaysWhichAppAndWhatItAskedFor() {
        // read off emulator-5554, API 36: the launcher in front, then Settings
        val launcher = "  mLastOrientationSource=WindowedMagnification:0:31@68443349\n    mLastOrientation=5\n"
        val settings = "    mLastOrientation=-1\n"
        assertEquals(5, Orientation.requestedOf(launcher))
        assertEquals(-1, Orientation.requestedOf(settings))
        assertEquals(null, Orientation.requestedOf("no such line"))
        val pinned = Orientation.didNotTakeSaw(0, 1, "com.example.app", 1)
        assertTrue(pinned, pinned.contains("com.example.app asks for a fixed orientation (portrait)"))
        val free = Orientation.didNotTakeSaw(0, 1, null, -1)
        assertTrue(free, free.contains("does not fix the orientation"))
        assertEquals(null, Orientation.fixedName(-1))
        assertEquals(null, Orientation.fixedName(4))
    }

    @Test
    fun aRefusedTurnNamesItselfForTheHost() {
        val not = JSONObject(RunnerWire.setOrientationBody(false, "landscapeLeft", 0, "the display stayed"))
        assertEquals("rotation_did_not_take", not.getString("error"))
        assertEquals("the display stayed", not.getString("saw"))
        assertFalse(JSONObject(RunnerWire.setOrientationBody(true, "portrait", 0)).has("error"))
    }
}
