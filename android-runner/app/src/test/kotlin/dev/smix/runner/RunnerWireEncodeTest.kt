// Response-body encode coverage for every route in SmixHttpServer.serve.
// Assertions parse the emitted string back through JSONObject — key
// order is not part of the wire contract.
//
// Contract cross-checks (crates/smix-runner-client/src/lib.rs):
// - /tap-by-id       → client reads `ok: bool`
// - /find-text-by-ocr → client reads `{found: bool, frame: [f64; 4]?}`
// - /system-popups   → client reads `{popups: [SystemPopup]}`
// - /system-popup-action → client reads `ok: bool`
// The coord/gesture routes (`/tap-at-norm-coord` etc.) are parsed as
// opaque serde_json::Value and discarded, so their bodies are
// Android-runner-owned shapes.

package dev.smix.runner

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RunnerWireEncodeTest {

    // MARK: - /health

    @Test
    fun healthEchoesVersion() {
        val obj = JSONObject(RunnerWire.healthBody("2.0.0"))
        assertEquals("ok", obj.getString("status"))
        assertEquals("smix-android-runner", obj.getString("runner"))
        assertEquals("2.0.0", obj.getString("version"))
    }

    // MARK: - /tap-at-norm-coord

    @Test
    fun tapAtNormCoordSuccessShape() {
        val obj = JSONObject(RunnerWire.tapAtNormCoordBody(true, 1080, 2400, 540, 1200))
        assertEquals("ok", obj.getString("status"))
        assertEquals(1080, obj.getInt("displayWidth"))
        assertEquals(2400, obj.getInt("displayHeight"))
        assertEquals(540, obj.getInt("x"))
        assertEquals(1200, obj.getInt("y"))
    }

    @Test
    fun tapAtNormCoordFailureStatus() {
        val obj = JSONObject(RunnerWire.tapAtNormCoordBody(false, 1080, 2400, 0, 0))
        assertEquals("click_returned_false", obj.getString("status"))
    }

    // MARK: - /swipe-at-norm-coord + /swipe-once

    @Test
    fun swipeAtNormCoordNestsFromTo() {
        val q = RunnerWire.SwipeQuad(10, 20, 30, 40)
        val obj = JSONObject(RunnerWire.swipeAtNormCoordBody(true, q))
        assertEquals("ok", obj.getString("status"))
        assertEquals(10, obj.getJSONObject("from").getInt("x"))
        assertEquals(20, obj.getJSONObject("from").getInt("y"))
        assertEquals(30, obj.getJSONObject("to").getInt("x"))
        assertEquals(40, obj.getJSONObject("to").getInt("y"))
    }

    @Test
    fun swipeOnceCarriesDirectionAndFailureStatus() {
        val q = RunnerWire.SwipeQuad(540, 1680, 540, 720)
        val obj = JSONObject(RunnerWire.swipeOnceBody(false, "down", q))
        assertEquals("swipe_returned_false", obj.getString("status"))
        assertEquals("down", obj.getString("direction"))
        assertEquals(1680, obj.getJSONObject("from").getInt("y"))
        assertEquals(720, obj.getJSONObject("to").getInt("y"))
    }

    // MARK: - /press-key /back /hide-keyboard /set-orientation

    @Test
    fun pressKeyEchoesKeyAndCode() {
        val obj = JSONObject(RunnerWire.pressKeyBody("return", 66))
        assertEquals("ok", obj.getString("status"))
        assertEquals("return", obj.getString("key"))
        assertEquals(66, obj.getInt("keyCode"))
    }

    @Test
    fun backStatusReflectsOutcome() {
        assertEquals("ok", JSONObject(RunnerWire.backBody(true)).getString("status"))
        assertEquals(
            "press_back_returned_false",
            JSONObject(RunnerWire.backBody(false)).getString("status"),
        )
    }

    @Test
    fun statusOkIsMinimal() {
        val obj = JSONObject(RunnerWire.statusOkBody())
        assertEquals("ok", obj.getString("status"))
        assertEquals(1, obj.length())
    }

    @Test
    fun setOrientationEchoesLiteral() {
        val obj = JSONObject(RunnerWire.setOrientationBody("landscapeRight"))
        assertEquals("ok", obj.getString("status"))
        assertEquals("landscapeRight", obj.getString("orientation"))
    }

    // MARK: - /tap-by-id (Rust client reads `ok`)

    @Test
    fun tapByIdOkTrueShape() {
        val obj = JSONObject(RunnerWire.tapByIdBody(true, "submit-btn", "a11y", true, true))
        assertTrue(obj.getBoolean("ok"))
        assertEquals("submit-btn", obj.getString("id"))
        assertEquals("a11y", obj.getString("path"))
        assertTrue(obj.getBoolean("saw_node"))
        assertTrue(obj.getBoolean("saw_action_click"))
    }

    @Test
    fun tapByIdOkFalseOnMiss() {
        val obj = JSONObject(RunnerWire.tapByIdBody(false, "ghost", "none", false, false))
        assertFalse(obj.getBoolean("ok"))
        assertEquals("none", obj.getString("path"))
    }

    // MARK: - /double-tap /long-press /input-text /foreground

    @Test
    fun doubleTapEchoesPixelCoord() {
        val obj = JSONObject(RunnerWire.doubleTapBody(540, 1200))
        assertEquals("ok", obj.getString("status"))
        assertEquals(540, obj.getInt("x"))
        assertEquals(1200, obj.getInt("y"))
    }

    @Test
    fun longPressEchoesDuration() {
        val obj = JSONObject(RunnerWire.longPressBody(540, 1200, 750L))
        assertEquals("ok", obj.getString("status"))
        assertEquals(750L, obj.getLong("durationMs"))
    }

    @Test
    fun inputTextEchoesUnescapedText() {
        val obj = JSONObject(RunnerWire.inputTextBody("hello world"))
        assertEquals("hello world", obj.getString("text"))
    }

    @Test
    fun foregroundEchoesBundleId() {
        val obj = JSONObject(RunnerWire.foregroundBody("com.example.app"))
        assertEquals("ok", obj.getString("status"))
        assertEquals("com.example.app", obj.getString("bundleId"))
    }

    // MARK: - /find-text-by-ocr (Rust client reads {found, frame})

    @Test
    fun ocrFoundNormalizesFrameToUnitSpace() {
        val obj = JSONObject(RunnerWire.ocrFoundBody(100, 400, 300, 500, 1000, 2000))
        assertTrue(obj.getBoolean("found"))
        val frame = obj.getJSONArray("frame")
        assertEquals(4, frame.length())
        assertEquals(0.1, frame.getDouble(0), 1e-9)
        assertEquals(0.2, frame.getDouble(1), 1e-9)
        assertEquals(0.2, frame.getDouble(2), 1e-9)
        assertEquals(0.05, frame.getDouble(3), 1e-9)
    }

    @Test
    fun ocrNotFoundOmitsFrame() {
        val obj = JSONObject(RunnerWire.ocrNotFoundBody())
        assertFalse(obj.getBoolean("found"))
        assertFalse(obj.has("frame"))
    }

    // MARK: - /system-popups + /system-popup-action

    @Test
    fun popupsBodyWrapsInPopupsEnvelope() {
        val popups = JSONArray().put(
            PopupWire.popupEntry("android-popup-0", "com.example.app", "Title", "Body", JSONArray()),
        )
        val obj = JSONObject(RunnerWire.popupsBody(popups))
        assertEquals(1, obj.getJSONArray("popups").length())
    }

    @Test
    fun popupActionOkReflectsClickOutcome() {
        val obj = JSONObject(RunnerWire.popupActionBody(true, "android-popup-0", "confirm-btn"))
        assertTrue(obj.getBoolean("ok"))
        assertEquals("android-popup-0", obj.getString("popupId"))
        assertEquals("confirm-btn", obj.getString("buttonId"))
        assertFalse(JSONObject(RunnerWire.popupActionBody(false, "p", "b")).getBoolean("ok"))
    }

    // MARK: - error envelopes

    @Test
    fun errorBodyShape() {
        val obj = JSONObject(RunnerWire.errorBody("bad_direction", "expected up/down"))
        assertEquals("bad_direction", obj.getString("error"))
        assertEquals("expected up/down", obj.getString("message"))
    }

    @Test
    fun internalErrorBodyShape() {
        val obj = JSONObject(RunnerWire.internalErrorBody("boom", "org.json.JSONException"))
        assertEquals("internal_error", obj.getString("error"))
        assertEquals("boom", obj.getString("message"))
        assertEquals("org.json.JSONException", obj.getString("class"))
    }

    @Test
    fun notImplementedBodyShape() {
        // Composed at runtime so route-conformance does not read the
        // deliberately-fake route as a phantom endpoint claim.
        val fakeRoute = "/" + "nope"
        val obj = JSONObject(RunnerWire.notImplementedBody(fakeRoute, "PUT"))
        assertEquals("not_implemented", obj.getString("error"))
        assertEquals(fakeRoute, obj.getString("route"))
        assertEquals("PUT", obj.getString("method"))
    }

    @Test
    fun proxyFailedBodyShape() {
        val obj = JSONObject(RunnerWire.proxyFailedBody("Connection refused"))
        assertEquals("proxy_failed", obj.getString("error"))
        assertEquals("Connection refused", obj.getString("message"))
        assertTrue(obj.getString("hint").contains("28081"))
    }

    // The Rust client reads `ok` on every act route (OkEnvelope). These
    // two answered with a `status` string instead — the same route, a
    // different shape from the iOS runner's `{"ok":bool}`, so success
    // and failure were indistinguishable to the host. Caught by running
    // a flow on an emulator, where /back reported ok while it had in
    // fact backgrounded the app.
    @Test
    fun backBodyCarriesTheOkFieldTheClientReads() {
        val ok = JSONObject(RunnerWire.backBody(true))
        assertEquals(true, ok.getBoolean("ok"))
        val bad = JSONObject(RunnerWire.backBody(false))
        assertEquals(false, bad.getBoolean("ok"))
    }

    @Test
    fun hideKeyboardBodyCarriesTheOkFieldTheClientReads() {
        val obj = JSONObject(RunnerWire.hideKeyboardBody(true))
        assertEquals(true, obj.getBoolean("ok"))
        assertEquals(false, JSONObject(RunnerWire.hideKeyboardBody(false)).getBoolean("ok"))
    }
}
