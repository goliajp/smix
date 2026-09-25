// Response-body encode coverage for every route in SmixHttpServer.serve.
// Assertions parse the emitted string back through JSONObject — key
// order is not part of the wire contract.
//
// Contract cross-checks (crates/smix-runner-client/src/lib.rs):
// - /tap-by-id       → client reads `ok: bool`
// - /find-text-by-ocr → client reads `{found: bool, frame: [f64; 4]?}`
// - /system-popups   → client reads `{popups: [SystemPopup]}`
// - /system-popup-action → client reads `ok: bool`
// - /tap-at-norm-coord, /swipe-at-norm-coord, /swipe-once, /press-key,
//   /double-tap-at-norm-coord, /long-press-at-norm-coord, /input-text,
//   /clear-text, /foreground, /set-orientation → client reads `ok: bool`
//   through OkEnvelope. It always did; until 10.2 these bodies did not
//   carry that field, and `OkEnvelope` reads an absent `ok` as success.

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

    // MARK: - /probe

    // The host puts the probe's tree in place of the app's windows and keeps
    // every other window from the accessibility reader. When the caller
    // named no app, the runner picked the one holding the focus; unless it
    // says which, the host cannot tell the app's windows from the rest.
    @Test
    fun aProbeThatIsThereSaysWhichAppItAnsweredFor() {
        val obj = JSONObject(RunnerWire.probePresentBody("dev.smix.fixture", "1", 2, 40L))
        assertTrue(obj.getBoolean("present"))
        assertEquals("dev.smix.fixture", obj.getString("app"))
        assertEquals("1", obj.getString("version"))
        assertEquals(2, obj.getInt("roots"))
        assertEquals(40L, obj.getLong("quietMs"))
    }

    // MARK: - /display

    @Test
    fun displayCarriesTheSizeAndHowManyPixelsMakeAPoint() {
        // A box compared in raw pixels would make `within: 1` a third of
        // a point on one phone and a whole one on another: the host needs
        // the density, and it needs it from the same place as the size.
        val obj = JSONObject(RunnerWire.displayBody(1080, 2340, 2.625f))
        assertEquals(1080, obj.getInt("width"))
        assertEquals(2340, obj.getInt("height"))
        assertEquals(2.625, obj.getDouble("pixelsPerPoint"), 1e-9)
    }

    // MARK: - /tap-at-norm-coord

    @Test
    fun tapAtNormCoordSuccessShape() {
        val obj = JSONObject(RunnerWire.tapAtNormCoordBody(true, 1080, 2400, 540, 1200, emptyList()))
        assertEquals("ok", obj.getString("status"))
        assertEquals(1080, obj.getInt("displayWidth"))
        assertEquals(2400, obj.getInt("displayHeight"))
        assertEquals(540, obj.getInt("x"))
        assertEquals(1200, obj.getInt("y"))
    }

    @Test
    fun tapAtNormCoordFailureStatus() {
        val obj = JSONObject(RunnerWire.tapAtNormCoordBody(false, 1080, 2400, 0, 0, emptyList()))
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
        val obj = JSONObject(RunnerWire.pressKeyBody(true, "return", 66))
        assertEquals("ok", obj.getString("status"))
        assertEquals("return", obj.getString("key"))
        assertEquals(66, obj.getInt("keyCode"))
    }

    @Test
    fun backStatusNamesTheBranchThatDecided() {
        val landed = JSONObject(
            RunnerWire.backBody(true, "screenChanged", "before=… last=…", injected = true),
        )
        assertEquals("ok", landed.getString("status"))
        // A refusal's `status` carries the branch rather than one fixed
        // phrase: "the key never went in" and "it went in and the
        // screen never changed" used to print the same sentence.
        val refused = JSONObject(
            RunnerWire.backBody(false, "gaveUp", "before=… last=…", injected = true),
        )
        assertEquals("gaveUp", refused.getString("status"))
        assertEquals("gaveUp", refused.getString("settledBy"))
        assertEquals(true, refused.getBoolean("injected"))
        assertTrue(refused.getString("saw").isNotEmpty())
    }

    @Test
    fun backSeparatesWhetherTheKeyWentInFromWhetherAnythingMoved() {
        // The two were one boolean until 10.2, and the one they were
        // was the wrong one.
        val notInjected = JSONObject(
            RunnerWire.backBody(false, "notInjected", "before=… last=<none>", injected = false),
        )
        assertEquals(false, notInjected.getBoolean("ok"))
        assertEquals(false, notInjected.getBoolean("injected"))
        val injectedButStill = JSONObject(
            RunnerWire.backBody(false, "gaveUp", "before=… last=…", injected = true),
        )
        assertEquals(false, injectedButStill.getBoolean("ok"))
        assertEquals(true, injectedButStill.getBoolean("injected"))
    }

    @Test
    fun everyOrientationNameHasOneRotationAndTheyAreAllDifferent() {
        val names = listOf("portrait", "landscapeLeft", "landscapeRight", "portraitUpsideDown")
        val rotations = names.map { RunnerWire.rotationFor(it) }
        assertEquals(listOf(0, 1, 3, 2), rotations)
        // Four names, four rotations: two names sharing one would make
        // the read-back unable to tell them apart.
        assertEquals(4, rotations.toSet().size)
        assertEquals(null, RunnerWire.rotationFor("sideways"))
        assertTrue(RunnerWire.rotationMatches("landscapeLeft", 1))
        assertFalse(RunnerWire.rotationMatches("landscapeLeft", 3))
    }

    @Test
    fun theRoutesThatUsedToAnswerStatusOkNowCarryWhatTheyDecided() {
        // Each of these built a body with `status: "ok"` in it and no
        // `ok` field at all, so a failure reached the host as a pass.
        assertEquals(false, JSONObject(RunnerWire.pressKeyBody(false, "return", 66)).getBoolean("ok"))
        assertEquals(false, JSONObject(RunnerWire.doubleTapBody(false, 1, 2, emptyList())).getBoolean("ok"))
        assertEquals(false, JSONObject(RunnerWire.longPressBody(false, 1, 2, 800L, emptyList())).getBoolean("ok"))
        assertEquals(false, JSONObject(RunnerWire.inputTextBody(false, "hi", "", "h", masked = false)).getBoolean("ok"))
        assertEquals(false, JSONObject(RunnerWire.clearTextBody(false, "key-events", 50, 3)).getBoolean("ok"))
        assertEquals(
            false,
            JSONObject(RunnerWire.foregroundBody(false, "dev.smix.fixture", "foreground=com.android.launcher3"))
                .getBoolean("ok"),
        )
        assertEquals(false, JSONObject(RunnerWire.setOrientationBody(false, "portrait", 1)).getBoolean("ok"))
        assertEquals(true, JSONObject(RunnerWire.tapAtNormCoordBody(true, 1080, 2400, 5, 6, emptyList())).getBoolean("ok"))
    }

    @Test
    fun clearTextSaysHowManyCharactersItFound() {
        // -1 is "I could not find the field to ask", which is not the
        // same answer as "the field is empty".
        val unknown = JSONObject(RunnerWire.clearTextBody(false, "set-text", 0, -1))
        assertEquals(-1, unknown.getInt("held"))
        assertEquals(false, unknown.getBoolean("ok"))
        // And the status says so. It said `field_not_empty`, which is a
        // claim about a field this route did not find.
        assertEquals("no_focused_field", unknown.getString("status"))
        assertEquals(
            "field_not_empty",
            JSONObject(RunnerWire.clearTextBody(false, "key-events", 64, 3)).getString("status"),
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
        val obj = JSONObject(RunnerWire.setOrientationBody(true, "landscapeRight", 3))
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
        val obj = JSONObject(RunnerWire.doubleTapBody(true, 540, 1200, emptyList()))
        assertEquals("ok", obj.getString("status"))
        assertEquals(540, obj.getInt("x"))
        assertEquals(1200, obj.getInt("y"))
    }

    @Test
    fun longPressEchoesDuration() {
        val obj = JSONObject(RunnerWire.longPressBody(true, 540, 1200, 750L, emptyList()))
        assertEquals("ok", obj.getString("status"))
        assertEquals(750L, obj.getLong("durationMs"))
    }

    @Test
    fun inputTextEchoesUnescapedText() {
        val obj = JSONObject(RunnerWire.inputTextBody(true, "hello world", "", "hello world", masked = false))
        assertEquals("hello world", obj.getString("text"))
    }

    @Test
    fun inputTextSaysWhatTheFieldHeldBeforeAndAfter() {
        // "It landed" alone let `mocmock@…` pass for
        // `mock@…`; the answer carries what the field held, so
        // the reader can see it rather than take the verdict's word.
        val plain = JSONObject(RunnerWire.inputTextBody(true, "abc", "x", "xabc", masked = false))
        assertEquals("x", plain.getString("before"))
        assertEquals("xabc", plain.getString("held"))
    }

    @Test
    fun aMaskedFieldIsReportedByLengthOnly() {
        val body = RunnerWire.inputTextBody(true, "Sunroom!24", "", "••••••••••", masked = true)
        val obj = JSONObject(body)
        assertEquals(0, obj.getInt("beforeLength"))
        assertEquals(10, obj.getInt("heldLength"))
        assertFalse("a masked field's reading went on the wire", obj.has("held") || obj.has("before"))
    }

    @Test
    fun foregroundEchoesBundleId() {
        val obj = JSONObject(RunnerWire.foregroundBody(true, "com.example.app", "foreground=com.example.app"))
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
        val ok = JSONObject(RunnerWire.backBody(true, "screenChanged", "…", injected = true))
        assertEquals(true, ok.getBoolean("ok"))
        val bad = JSONObject(RunnerWire.backBody(false, "gaveUp", "…", injected = true))
        assertEquals(false, bad.getBoolean("ok"))
    }

    @Test
    fun hideKeyboardBodyCarriesTheOkFieldTheClientReads() {
        val obj = JSONObject(RunnerWire.hideKeyboardBody(true))
        assertEquals(true, obj.getBoolean("ok"))
        assertEquals(false, JSONObject(RunnerWire.hideKeyboardBody(false)).getBoolean("ok"))
    }
}

// --- the keyboard is a thing you can ask about, on both platforms -------
//
// `role:keyboard` answers on iOS — Apple types the software keyboard 19
// and the tree carries it, so `extendedWaitUntil { visible: { role:
// keyboard } }` has always worked there. Measured on the fixture: six
// steps, keyboard in and out, all green.
//
// On Android the same flow timed out with ELEMENT_NOT_FOUND while the
// keyboard was unmistakably on screen — screenshot and `/windows` both
// said so. The runner already knew: `keyboardIsUp()` reads exactly the
// window type below and `hide-keyboard` has been deciding on it for
// releases. Nothing lifted it into the tree, so no verb could see it.
//
// A consumer read that as "there is no way to wait for the keyboard" and
// reached for a pause instead. Half of that was right.

class KeyboardRoleTest {
    @Test
    fun anInputMethodWindowIsTheKeyboard() {
        // AccessibilityWindowInfo.TYPE_INPUT_METHOD == 2. The literal is
        // used rather than the constant because this test runs on the
        // JVM, where the android framework classes are stubs — and the
        // wire value is the thing that has to stay put anyway.
        assertEquals("keyboard", TreeWire.roleForWindowType(2))
    }

    @Test
    fun everyOtherWindowKeepsWhateverItsClassSaid() {
        // 1 = application, 3 = system, 4 = accessibility overlay. None
        // of them is a keyboard, and a role invented here would override
        // the one derived from the node's class.
        for (t in listOf(1, 3, 4, 5, 6)) {
            assertEquals("window type $t", null, TreeWire.roleForWindowType(t))
        }
    }

    @Test
    fun aTapCarriesWhatItWasDeliveredToAndSaysTheListIsWhole() {
        val chain = listOf(
            HitChain.Entry("button1", "", HitChain.Box(773, 1192, 976, 1341)),
            HitChain.Entry("", "", HitChain.Box(0, 0, 1080, 2340)),
        )
        val obj = JSONObject(RunnerWire.tapAtNormCoordBody(true, 1080, 2340, 874, 1266, chain))
        val arr = obj.getJSONArray("chain")
        assertEquals(2, arr.length())
        assertEquals("button1", arr.getJSONObject(0).getString("identifier"))
        val frame = arr.getJSONObject(0).getJSONObject("frame")
        assertEquals(listOf(773, 1192, 203, 149), listOf("x", "y", "w", "h").map { frame.getInt(it) })
        assertEquals("", arr.getJSONObject(1).getString("identifier"))
        assertEquals(true, obj.getBoolean("complete"))
        // The same for the two other touches aimed by a selector.
        assertEquals(2, JSONObject(RunnerWire.doubleTapBody(true, 1, 2, chain)).getJSONArray("chain").length())
        assertEquals(2, JSONObject(RunnerWire.longPressBody(true, 1, 2, 800L, chain)).getJSONArray("chain").length())
    }
}
