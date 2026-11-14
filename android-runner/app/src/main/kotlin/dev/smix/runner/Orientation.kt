// Which display rotation each orientation name means on this device,
// and how /set-orientation gets the display there.
//
// Free of android.* imports so it runs as a JVM unit test.
package dev.smix.runner

import org.json.JSONObject

object Orientation {
    /// Whether the display is landscape at rotation 0, from its size and
    /// rotation as they are now. A tablet's natural axis is landscape, a
    /// phone's is portrait; a square display counts as portrait.
    fun naturalIsLandscape(width: Int, height: Int, rotation: Int): Boolean =
        if (rotation % 2 == 0) width > height else height > width

    /// The `Surface.ROTATION_*` that leaves the display in `orientation`.
    ///
    /// Rotation counts quarter turns from the natural axis, not from
    /// portrait. On a portrait-natural phone (measured, emulator-5554,
    /// API 36) portrait is 0, landscapeLeft 1, landscapeRight 3, upside
    /// down 2. On a landscape-natural tablet rotation 0 is landscape, and
    /// portrait is 3 — the window manager's own portrait rotation there
    /// unless the device sets `config_reverseDefaultRotation` — with each
    /// other name the same quarter turn away from portrait as on a phone.
    /// Asking a tablet for rotation 0 as "portrait" left it in landscape,
    /// and the read-back agreed with the wrong number.
    fun rotationFor(orientation: String, naturalIsLandscape: Boolean): Int? {
        val fromPortrait = when (orientation) {
            "portrait" -> 0
            "landscapeLeft" -> 1
            "landscapeRight" -> 3
            "portraitUpsideDown" -> 2
            else -> return null
        }
        return if (naturalIsLandscape) (fromPortrait + 3) % 4 else fromPortrait
    }

    /// How the route reaches a rotation. UiAutomator freezes 0, 1 and 3
    /// and has no call for 2; its old emulation of 2 (natural, then two
    /// left turns) left emulator-5554 at rotation 1. The settings route
    /// reaches 2, with the accelerometer frozen first.
    enum class Turn { NATURAL, LEFT, RIGHT, SETTINGS }

    fun turnFor(rotation: Int): Turn = when (rotation) {
        0 -> Turn.NATURAL
        1 -> Turn.LEFT
        3 -> Turn.RIGHT
        else -> Turn.SETTINGS
    }

    fun settingsCommands(rotation: Int): List<String> = listOf(
        "settings put system accelerometer_rotation 0",
        "settings put system user_rotation $rotation",
    )

    /// The rotation the display was left in, and whether it is the one
    /// asked for.
    fun body(ok: Boolean, orientation: String, rotation: Int): String = JSONObject()
        .put("ok", ok)
        .put("status", if (ok) "ok" else "rotation_did_not_take")
        .put("orientation", orientation)
        .put("rotation", rotation)
        .toString()
}
