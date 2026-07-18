// Request-body decode coverage for every POST route in
// SmixHttpServer.serve. Malformed bodies must throw JSONException —
// serve()'s catch-all turns that into the 500 internal_error envelope.

package dev.smix.runner

import org.json.JSONException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class RunnerWireDecodeTest {

    // MARK: - /tap-at-norm-coord + /double-tap-at-norm-coord

    @Test
    fun normCoordDecodesBothAxes() {
        val req = RunnerWire.decodeNormCoord("""{"nx":0.25,"ny":0.75}""")
        assertEquals(0.25, req.nx, 0.0)
        assertEquals(0.75, req.ny, 0.0)
    }

    @Test
    fun normCoordMissingNyThrows() {
        assertThrows(JSONException::class.java) {
            RunnerWire.decodeNormCoord("""{"nx":0.25}""")
        }
    }

    @Test
    fun normCoordNonJsonThrows() {
        assertThrows(JSONException::class.java) {
            RunnerWire.decodeNormCoord("not json")
        }
    }

    @Test
    fun normCoordEmptyBodyThrows() {
        assertThrows(JSONException::class.java) {
            RunnerWire.decodeNormCoord("{}")
        }
    }

    // MARK: - /swipe-at-norm-coord

    @Test
    fun swipeAtNormCoordDecodesAllFourFields() {
        val req = RunnerWire.decodeSwipeAtNormCoord(
            """{"fromNx":0.1,"fromNy":0.2,"toNx":0.3,"toNy":0.4}""",
        )
        assertEquals(0.1, req.fromNx, 0.0)
        assertEquals(0.2, req.fromNy, 0.0)
        assertEquals(0.3, req.toNx, 0.0)
        assertEquals(0.4, req.toNy, 0.0)
    }

    @Test
    fun swipeAtNormCoordMissingToNyThrows() {
        assertThrows(JSONException::class.java) {
            RunnerWire.decodeSwipeAtNormCoord("""{"fromNx":0.1,"fromNy":0.2,"toNx":0.3}""")
        }
    }

    // MARK: - /swipe-once

    @Test
    fun swipeOnceDecodesDirection() {
        assertEquals("down", RunnerWire.decodeSwipeOnce("""{"direction":"down"}"""))
    }

    @Test
    fun swipeOnceMissingDirectionThrows() {
        assertThrows(JSONException::class.java) {
            RunnerWire.decodeSwipeOnce("{}")
        }
    }

    // MARK: - /press-key

    @Test
    fun pressKeyDecodesKey() {
        assertEquals("return", RunnerWire.decodePressKey("""{"key":"return"}"""))
    }

    @Test
    fun pressKeyMissingKeyThrows() {
        assertThrows(JSONException::class.java) {
            RunnerWire.decodePressKey("{}")
        }
    }

    // MARK: - /set-orientation

    @Test
    fun setOrientationDecodesLiteral() {
        assertEquals(
            "landscapeLeft",
            RunnerWire.decodeSetOrientation("""{"orientation":"landscapeLeft"}"""),
        )
    }

    // MARK: - /tap-by-id

    @Test
    fun tapByIdDecodesId() {
        assertEquals("submit-btn", RunnerWire.decodeTapById("""{"id":"submit-btn"}"""))
    }

    @Test
    fun tapByIdMissingIdThrows() {
        assertThrows(JSONException::class.java) {
            RunnerWire.decodeTapById("{}")
        }
    }

    // MARK: - /long-press-at-norm-coord

    @Test
    fun longPressDecodesExplicitDuration() {
        val req = RunnerWire.decodeLongPressAtNormCoord(
            """{"nx":0.5,"ny":0.5,"durationMs":1200}""",
        )
        assertEquals(0.5, req.nx, 0.0)
        assertEquals(0.5, req.ny, 0.0)
        assertEquals(1200L, req.durationMs)
    }

    @Test
    fun longPressDurationDefaultsTo500() {
        val req = RunnerWire.decodeLongPressAtNormCoord("""{"nx":0.5,"ny":0.5}""")
        assertEquals(500L, req.durationMs)
    }

    @Test
    fun longPressMissingNxThrows() {
        assertThrows(JSONException::class.java) {
            RunnerWire.decodeLongPressAtNormCoord("""{"ny":0.5}""")
        }
    }

    // MARK: - /input-text

    @Test
    fun inputTextDecodesText() {
        assertEquals("hello world", RunnerWire.decodeInputText("""{"text":"hello world"}"""))
    }

    @Test
    fun inputTextMissingTextThrows() {
        assertThrows(JSONException::class.java) {
            RunnerWire.decodeInputText("{}")
        }
    }

    // MARK: - /foreground

    @Test
    fun foregroundDecodesBundleId() {
        assertEquals(
            "com.example.app",
            RunnerWire.decodeForeground("""{"bundleId":"com.example.app"}"""),
        )
    }

    @Test
    fun foregroundMissingBundleIdThrows() {
        assertThrows(JSONException::class.java) {
            RunnerWire.decodeForeground("{}")
        }
    }

    // MARK: - /find-text-by-ocr

    @Test
    fun ocrTargetDecodesTextIgnoringVisionArgs() {
        assertEquals(
            "Sign In",
            RunnerWire.decodeOcrTarget(
                """{"text":"Sign In","locales":["en"],"recognition_level":"accurate"}""",
            ),
        )
    }

    @Test
    fun ocrTargetMissingTextThrows() {
        assertThrows(JSONException::class.java) {
            RunnerWire.decodeOcrTarget("""{"locales":["en"]}""")
        }
    }

    @Test
    fun systemPopupAction_readsTheCamelCaseKeysTheRustClientSends() {
        val req = RunnerWire.decodeSystemPopupAction(
            """{"popupId":"android-popup-0","buttonId":"allow"}"""
        )
        assertEquals("android-popup-0", req.popupId)
        assertEquals("allow", req.buttonId)
    }

    @Test(expected = org.json.JSONException::class)
    fun systemPopupAction_rejectsTheOldSnakeCaseKeys() {
        // The route read popup_id/button_id for as long as it existed,
        // 500ing every real client request. camelCase is the contract.
        RunnerWire.decodeSystemPopupAction("""{"popup_id":"p","button_id":"b"}""")
    }
}
