// Pure wire logic for the Android runner HTTP surface: request-body
// decode + response-body encode for the SmixHttpServer routes in
// RunnerTest.kt (androidTest).
//
// Deliberately free of android.* imports so the whole file runs as a
// plain JVM unit test (`:app:testDebugUnitTest`). org.json comes from
// the Android framework at runtime and from the org.json:json test
// dependency on the JVM.
//
// Decode functions throw org.json.JSONException on malformed bodies;
// SmixHttpServer.serve's catch-all converts that to the 500
// internal_error envelope.

package dev.smix.runner

import org.json.JSONArray
import org.json.JSONObject

object RunnerWire {
    // ---- request decode ----

    data class NormCoord(val nx: Double, val ny: Double)

    fun decodeNormCoord(payload: String): NormCoord {
        val req = JSONObject(payload)
        return NormCoord(req.getDouble("nx"), req.getDouble("ny"))
    }

    data class SwipeCoords(
        val fromNx: Double,
        val fromNy: Double,
        val toNx: Double,
        val toNy: Double,
    )

    fun decodeSwipeAtNormCoord(payload: String): SwipeCoords {
        val req = JSONObject(payload)
        return SwipeCoords(
            req.getDouble("fromNx"),
            req.getDouble("fromNy"),
            req.getDouble("toNx"),
            req.getDouble("toNy"),
        )
    }

    fun decodeSwipeOnce(payload: String): String =
        JSONObject(payload).getString("direction")

    fun decodePressKey(payload: String): String =
        JSONObject(payload).getString("key")

    fun decodeSetOrientation(payload: String): String =
        JSONObject(payload).getString("orientation")

    fun decodeTapById(payload: String): String =
        JSONObject(payload).getString("id")

    data class LongPressReq(val nx: Double, val ny: Double, val durationMs: Long)

    fun decodeLongPressAtNormCoord(payload: String): LongPressReq {
        val req = JSONObject(payload)
        return LongPressReq(
            req.getDouble("nx"),
            req.getDouble("ny"),
            req.optLong("durationMs", 500L),
        )
    }

    /// What to type, and — when the caller named a field — where they
    /// tapped to focus it.
    ///
    /// `focusAt` absent means "wherever focus is". That is what a bare
    /// `inputText` step means, and what a client older than this field
    /// sends, so it is a decision rather than an omission; see
    /// `focusAccepts`.
    data class InputTextRequest(val text: String, val focusAt: NormCoord?)

    fun decodeInputText(payload: String): InputTextRequest {
        val req = JSONObject(payload)
        return InputTextRequest(req.getString("text"), focusPoint(req))
    }

    /// The clear that precedes a fill needs the same target, or it
    /// empties the field that happened to have focus — measured on
    /// emulator-5554, a fill naming one field erased another.
    fun decodeClearText(payload: String): NormCoord? =
        if (payload.isBlank()) null else focusPoint(JSONObject(payload))

    private fun focusPoint(req: JSONObject): NormCoord? =
        if (req.has("focusNx") && req.has("focusNy")) {
            NormCoord(req.getDouble("focusNx"), req.getDouble("focusNy"))
        } else {
            null
        }

    /// Screen-pixel containment, edges included. A tap on the boundary
    /// focuses the field, so a point on the boundary belongs to it.
    fun nodeHoldsPoint(
        left: Int,
        top: Int,
        right: Int,
        bottom: Int,
        x: Int,
        y: Int,
    ): Boolean = x in left..right && y in top..bottom

    /// Is this focused field the one the caller meant?
    ///
    /// With no point, yes — every focused field qualifies, which is the
    /// documented meaning of a fill with no selector. With a point,
    /// only the field holding it. The two halves belong together: the
    /// permissive one alone would be a hole rather than a rule.
    fun focusAccepts(
        left: Int,
        top: Int,
        right: Int,
        bottom: Int,
        atX: Int?,
        atY: Int?,
    ): Boolean =
        if (atX == null || atY == null) {
            true
        } else {
            nodeHoldsPoint(left, top, right, bottom, atX, atY)
        }

    fun decodeForeground(payload: String): String =
        JSONObject(payload).getString("bundleId")

    fun decodeOcrTarget(payload: String): String =
        JSONObject(payload).getString("text")

    // Keys are the Rust contract (SessionOpenRequest, camelCase).
    // `bundleId` / `activate` both carry #[serde(default)] on the Rust
    // side, so absent keys decode to ""/false rather than erroring; the
    // handler turns an empty bundleId into the 400 bad_request envelope
    // (mirrors the iOS SessionRoute.decodeOpen emptyBundleId path).
    data class SessionOpenReq(val bundleId: String, val activate: Boolean)

    fun decodeSessionOpen(payload: String): SessionOpenReq {
        val req = JSONObject(payload)
        return SessionOpenReq(
            req.optString("bundleId", ""),
            req.optBoolean("activate", false),
        )
    }

    // Shared by /session/close, /session/renew-activation,
    // /session/relaunch-app, /session/terminate-app, /session/launch-app
    // — every Rust Session*Request carries `sessionId` (camelCase). The
    // lifecycle routes additionally send args/env/waitForForegroundMs/
    // waitForInteractiveMs (XCUITest-specific launch injection); Android
    // accepts and ignores them. Missing/empty sessionId decodes to ""
    // and the handler emits the 400 bad_request envelope.
    fun decodeSessionId(payload: String): String =
        JSONObject(payload).optString("sessionId", "")

    data class PopupActionReq(val popupId: String, val buttonId: String)

    // Keys are the Rust contract (SystemPopupActionRequest, camelCase).
    // This read used snake_case for as long as the route existed, so the
    // real client always got a 500 and popup-button taps never worked on
    // Android.
    fun decodeSystemPopupAction(payload: String): PopupActionReq {
        val req = JSONObject(payload)
        return PopupActionReq(req.getString("popupId"), req.getString("buttonId"))
    }

    // ---- pure transforms ----

    fun normToPixel(n: Double, extent: Int): Int =
        (n * extent).toInt().coerceIn(0, extent - 1)

    data class SwipeQuad(val x1: Int, val y1: Int, val x2: Int, val y2: Int)

    // Maestro navigation convention (`SwipeDirection` enum docstring):
    // the wire "direction" names what content to SEE, not the finger
    // gesture direction. `down` = "navigate down" = "see below" =
    // content moves up = finger gestures up (start at y=70%, end at
    // y=30%). 30%-70% of the screen along the swipe axis; cross-axis
    // fixed at midline.
    fun swipeOnceCoords(direction: String, w: Int, h: Int): SwipeQuad? = when (direction) {
        // see above ← finger gestures down
        "up" -> SwipeQuad(w / 2, (h * 0.3).toInt(), w / 2, (h * 0.7).toInt())
        // see below ← finger gestures up
        "down" -> SwipeQuad(w / 2, (h * 0.7).toInt(), w / 2, (h * 0.3).toInt())
        // see left  ← finger gestures right
        "left" -> SwipeQuad((w * 0.3).toInt(), h / 2, (w * 0.7).toInt(), h / 2)
        // see right ← finger gestures left
        "right" -> SwipeQuad((w * 0.7).toInt(), h / 2, (w * 0.3).toInt(), h / 2)
        else -> null
    }

    // UiDevice.swipe(x, y, x, y, steps) — same start/end coord with
    // explicit step count approximates a long press; each step adds
    // ~5ms, so steps = duration / 5 ms.
    fun longPressSteps(durationMs: Long): Int =
        (durationMs / 5).toInt().coerceAtLeast(1)

    // Android `input text` interprets unescaped spaces as separators.
    // UiAutomation.executeShellCommand does NOT run through `sh -c`; it
    // splits on whitespace + execs directly, so quote characters are
    // typed literally. Use bare `input text` + escape spaces to `%s`
    // (input cmd convention).
    fun escapeForInputText(text: String): String =
        text.replace("\\", "\\\\").replace(" ", "%s")

    fun inputTextCommand(text: String): String =
        "input text ${escapeForInputText(text)}"

    // The activity is resolved by the caller, which has a Context and
    // can ask the package manager. `.MainActivity` remains only as the
    // answer of last resort: it is what a scaffolded app is called and
    // what almost nothing else is, so every launch used to work for
    // apps generated from a template and silently for no others.
    const val ACTIVITY_CONVENTION: String = ".MainActivity"

    fun foregroundCommand(bundleId: String, activity: String? = null): String =
        "am start --activity-single-top -n $bundleId/${activity ?: ACTIVITY_CONVENTION}"

    // /session/launch-app + /session/relaunch-app reuse the /foreground
    // entry-point resolution — same launch semantics, so the same
    // question about which activity to start.
    fun terminateAppCommand(bundleId: String): String =
        "am force-stop $bundleId"

    // ---- response encode ----

    // `ok` + `runnerVersion` are what the Rust HealthResponse reads;
    // `status`/`runner`/`version` stay for existing shell probes.
    fun healthBody(version: String): String = JSONObject()
        .put("ok", true)
        .put("status", "ok")
        .put("runner", "smix-android-runner")
        .put("version", version)
        .put("runnerVersion", version)
        .toString()

    fun tapAtNormCoordBody(ok: Boolean, displayWidth: Int, displayHeight: Int, x: Int, y: Int): String =
        JSONObject()
            .put("status", if (ok) "ok" else "click_returned_false")
            .put("displayWidth", displayWidth)
            .put("displayHeight", displayHeight)
            .put("x", x)
            .put("y", y)
            .toString()

    fun swipeAtNormCoordBody(ok: Boolean, q: SwipeQuad): String = JSONObject()
        .put("status", if (ok) "ok" else "swipe_returned_false")
        .put("from", JSONObject().put("x", q.x1).put("y", q.y1))
        .put("to", JSONObject().put("x", q.x2).put("y", q.y2))
        .toString()

    fun swipeOnceBody(ok: Boolean, direction: String, q: SwipeQuad): String = JSONObject()
        .put("status", if (ok) "ok" else "swipe_returned_false")
        .put("direction", direction)
        .put("from", JSONObject().put("x", q.x1).put("y", q.y1))
        .put("to", JSONObject().put("x", q.x2).put("y", q.y2))
        .toString()

    fun pressKeyBody(key: String, keyCode: Int): String = JSONObject()
        .put("status", "ok")
        .put("key", key)
        .put("keyCode", keyCode)
        .toString()

    // `ok` is the field the Rust client reads on every act route
    // (OkEnvelope) and the shape the iOS runner emits. These two used to
    // answer with a `status` string, so their success and failure looked
    // identical to the host. `status` stays alongside for shell probes.
    fun backBody(ok: Boolean): String = JSONObject()
        .put("ok", ok)
        .put("status", if (ok) "ok" else "press_back_returned_false")
        .toString()

    fun hideKeyboardBody(ok: Boolean): String = JSONObject()
        .put("ok", ok)
        .put("status", if (ok) "ok" else "press_back_returned_false")
        .toString()

    fun statusOkBody(): String = JSONObject().put("status", "ok").toString()

    fun setOrientationBody(orientation: String): String = JSONObject()
        .put("status", "ok")
        .put("orientation", orientation)
        .toString()

    fun tapByIdBody(
        ok: Boolean,
        id: String,
        path: String,
        sawNode: Boolean,
        sawActionClick: Boolean,
    ): String = JSONObject()
        .put("ok", ok)
        .put("id", id)
        .put("path", path)
        .put("saw_node", sawNode)
        .put("saw_action_click", sawActionClick)
        .toString()

    fun doubleTapBody(x: Int, y: Int): String = JSONObject()
        .put("status", "ok")
        .put("x", x)
        .put("y", y)
        .toString()

    fun longPressBody(x: Int, y: Int, durationMs: Long): String = JSONObject()
        .put("status", "ok")
        .put("x", x)
        .put("y", y)
        .put("durationMs", durationMs)
        .toString()

    fun inputTextBody(text: String): String = JSONObject()
        .put("status", "ok")
        .put("text", text)
        .toString()

    // `input keyevent` takes any number of keycodes in one invocation,
    // so a fallback clear costs one shell exec rather than one per
    // character. 67 is KEYCODE_DEL.
    fun deleteKeysCommand(count: Int): String =
        "input keyevent" + " 67".repeat(maxOf(count, 1))

    // `method` says how the field was emptied, because the two are not
    // equally trustworthy: `set-text` is exact, `key-events` deletes a
    // bounded number of characters and can leave a longer field
    // partly filled. A caller that cannot tell them apart cannot know
    // which it got.
    fun clearTextBody(method: String, deletes: Int): String = JSONObject()
        .put("status", "ok")
        .put("method", method)
        .put("deletes", deletes)
        .toString()

    fun foregroundBody(bundleId: String): String = JSONObject()
        .put("status", "ok")
        .put("bundleId", bundleId)
        .toString()

    // Wire shape matches iOS swift /find-text-by-ocr response:
    // {found: bool, frame: [nx, ny, w, h]} per HttpRunnerClient
    // deserialization.
    fun ocrFoundBody(left: Int, top: Int, right: Int, bottom: Int, imageWidth: Int, imageHeight: Int): String {
        val nx = left.toDouble() / imageWidth
        val ny = top.toDouble() / imageHeight
        val w = (right - left).toDouble() / imageWidth
        val h = (bottom - top).toDouble() / imageHeight
        return JSONObject()
            .put("found", true)
            .put("frame", JSONArray().put(nx).put(ny).put(w).put(h))
            .toString()
    }

    fun ocrNotFoundBody(): String = JSONObject().put("found", false).toString()

    fun popupsBody(popups: JSONArray): String =
        JSONObject().put("popups", popups).toString()

    fun popupActionBody(ok: Boolean, popupId: String, buttonId: String): String = JSONObject()
        .put("ok", ok)
        .put("popupId", popupId)
        .put("buttonId", buttonId)
        .toString()

    // ---- /session/* response encode ----
    //
    // Key names lock to the Rust smix-runner-wire structs
    // (SessionOpenResponse / SessionCloseResponse /
    // SessionCloseAllResponse / SessionRenewActivationResponse /
    // SessionListResponse+SessionSummary / SessionAppLifecycleResponse /
    // SessionRelaunchAppResponse — all camelCase), same contract the iOS
    // SessionRoute emitters are gated against in SessionRouteTests.

    // `ok` is not in the Rust SessionOpenResponse (serde ignores it);
    // kept for shell-probe symmetry with the other Android bodies.
    fun sessionOpenBody(sessionId: String, activatedOnce: Boolean, serverTimeMs: Long): String =
        JSONObject()
            .put("ok", true)
            .put("sessionId", sessionId)
            .put("activatedOnce", activatedOnce)
            .put("serverTimeMs", serverTimeMs)
            .toString()

    // Idempotent — closing an unknown/already-closed session is
    // `ok:true`, per the SessionCloseRequest contract. NOT the
    // not-found envelope (that's renew/lifecycle/relaunch only).
    fun sessionCloseBody(): String = JSONObject().put("ok", true).toString()

    fun sessionCloseAllBody(closed: Int): String = JSONObject()
        .put("ok", true)
        .put("closed", closed)
        .toString()

    fun sessionRenewBody(activated: Boolean): String = JSONObject()
        .put("ok", true)
        .put("activated", activated)
        .toString()

    fun sessionListBody(sessions: List<SessionTable.Entry>): String {
        val arr = JSONArray()
        for (e in sessions) {
            arr.put(
                JSONObject()
                    .put("sessionId", e.sessionId)
                    .put("bundleId", e.bundleId)
                    .put("openedAtMs", e.openedAtMs)
                    .put("lastActivatedAtMs", e.lastActivatedAtMs)
                    // Interactive-probe is XCUITest-side machinery the
                    // Android runner doesn't implement; empty array is
                    // the contract-legal "no sample" value.
                    .put("interactiveNamedIds", JSONArray()),
            )
        }
        return JSONObject().put("sessions", arr).toString()
    }

    // /session/terminate-app + /session/launch-app. waitedMs /
    // terminalState / terminatedCooperatively / reachedInteractive are
    // XCUIApplication.state semantics with no `am` equivalent — emitted
    // as their Rust serde-default values for byte-shape parity with the
    // iOS appLifecycleResponse.
    fun sessionLifecycleBody(ok: Boolean, wallMs: Long): String = JSONObject()
        .put("ok", ok)
        .put("wallMs", wallMs)
        .put("waitedMs", 0)
        .put("terminalState", 0)
        .put("terminatedCooperatively", false)
        .put("reachedInteractive", false)
        .put("interactiveNamedIds", JSONArray())
        .toString()

    fun sessionRelaunchBody(ok: Boolean, wallMs: Long): String = JSONObject()
        .put("ok", ok)
        .put("wallMs", wallMs)
        .toString()

    // 404/400 envelopes — same shape the iOS SessionRoute.notFound /
    // .badRequest emit: {"ok":false,"error":kind,"reason":reason}.
    fun sessionNotFoundBody(reason: String): String = JSONObject()
        .put("ok", false)
        .put("error", "not_found")
        .put("reason", reason)
        .toString()

    fun sessionBadRequestBody(reason: String): String = JSONObject()
        .put("ok", false)
        .put("error", "bad_request")
        .put("reason", reason)
        .toString()

    fun errorBody(kind: String, message: String): String = JSONObject()
        .put("error", kind)
        .put("message", message)
        .toString()

    fun internalErrorBody(message: String, className: String): String = JSONObject()
        .put("error", "internal_error")
        .put("message", message)
        .put("class", className)
        .toString()

    fun notImplementedBody(route: String, method: String): String = JSONObject()
        .put("error", "not_implemented")
        .put("route", route)
        .put("method", method)
        .toString()

    fun proxyFailedBody(message: String): String = JSONObject()
        .put("error", "proxy_failed")
        .put("message", message)
        .put("hint", "ensure the app's WebViewEvalServer is up on :28081")
        .toString()

    /// Resource-id spellings to try for a short id, most likely first.
    ///
    /// Compose with `testTagsAsResourceId = true` emits the bare short
    /// string on some layouts (FlowRow) and `<pkg>:id/<short>` on others
    /// (older LazyRow), so both spellings have to be attempted.
    ///
    /// The package-qualified form needs the package of the app under
    /// test, which only the caller knows — it arrives on the
    /// `App-Bundle-Id` header. Until that was wired, this list carried
    /// the literal `com.example.app`, which is the README's placeholder
    /// and not any real app: the qualified spelling could only ever
    /// match a reader who had copied the example verbatim. Every real
    /// app fell through to the manual walk, slower and by a different
    /// code path than the one the comment described.
    ///
    /// The runner's own test process stays in the list: it addresses
    /// its fixtures by id in the self-tests.
    fun viewIdCandidates(shortId: String, targetPackage: String?): List<String> = buildList {
        add(shortId)
        if (!targetPackage.isNullOrBlank()) {
            add("$targetPackage:id/$shortId")
        }
        add("dev.smix.runner.test:id/$shortId")
    }

    /// Name the spelling that actually found the node.
    ///
    /// Reads beside viewIdCandidates on purpose: one decides which
    /// spellings to try, this one decides how to say which was tried
    /// successfully, and neither is complete without the other.
    ///
    /// It exists because the strict lookup and the manual walk return
    /// the same node. That is how `com.example.app` — a documentation
    /// placeholder sitting in the candidate list as though it were a
    /// real package — lasted through releases: every real app missed
    /// the qualified spelling and fell through to the walk, which
    /// answered identically. Nothing failed, so nothing was noticed.
    ///
    /// `matched` is the candidate the framework lookup hit, or null
    /// when all of them missed and the walk took over.
    fun viewIdMatchKind(matched: String?, shortId: String, targetPackage: String?): String =
        when {
            matched == null -> "walk"
            matched == shortId -> "bare"
            !targetPackage.isNullOrBlank() && matched == "$targetPackage:id/$shortId" -> "qualified"
            matched == "dev.smix.runner.test:id/$shortId" -> "runner-test"
            else -> "other-package"
        }

    /// Did the characters reach the field?
    ///
    /// A plain field can be asked directly: its accessibility node
    /// carries what it holds, so the honest evidence is that the node
    /// now contains what was typed.
    ///
    /// A masked field cannot answer that question at all. Its node
    /// reports one bullet per character and never the characters, so
    /// `contains` is false for every fill that ever worked — the
    /// verdict 6.4.0 shipped, and the one that stopped a consumer's
    /// twenty-flow suite at the flow that signs in. What a mask can
    /// still tell you is how much longer it got, so that is what is
    /// asked of it.
    ///
    /// `isPassword` comes from the node saying so
    /// (`AccessibilityNodeInfo.isPassword`), never from the text
    /// looking like a mask. A field holding `aaaa` is not masked, and a
    /// predicate that guessed would quietly stop checking content for
    /// anyone whose password repeats a character.
    fun textLanded(
        before: String,
        after: String,
        dispatched: String,
        isPassword: Boolean,
    ): Boolean =
        if (isPassword) {
            after.length - before.length == dispatched.length
        } else {
            after.contains(dispatched)
        }
}

/// Map smix KeyName camelCase (per `smix_input::KeyName` serde rename) →
/// Android KeyEvent.KEYCODE_*. Returns null when no clean mapping.
///
/// Literal keycodes instead of android.view.KeyEvent constants keep this
/// file android.*-free; KeyMapTest cross-checks every literal against the
/// real KeyEvent constants.
object KeyMap {
    fun androidKeyCode(name: String): Int? = when (name) {
        "return" -> 66 // KeyEvent.KEYCODE_ENTER
        "delete" -> 67 // KeyEvent.KEYCODE_DEL
        "tab" -> 61 // KeyEvent.KEYCODE_TAB
        "space" -> 62 // KeyEvent.KEYCODE_SPACE
        "escape" -> 111 // KeyEvent.KEYCODE_ESCAPE
        "arrowUp" -> 19 // KeyEvent.KEYCODE_DPAD_UP
        "arrowDown" -> 20 // KeyEvent.KEYCODE_DPAD_DOWN
        "arrowLeft" -> 21 // KeyEvent.KEYCODE_DPAD_LEFT
        "arrowRight" -> 22 // KeyEvent.KEYCODE_DPAD_RIGHT
        "home" -> 3 // KeyEvent.KEYCODE_HOME
        "lock" -> 26 // KeyEvent.KEYCODE_POWER
        "volumeUp" -> 24 // KeyEvent.KEYCODE_VOLUME_UP
        "volumeDown" -> 25 // KeyEvent.KEYCODE_VOLUME_DOWN
        else -> null
    }
}

/// Pure JSON assembly for /system-popups + /system-popup-action.
/// The a11y-tree walking stays in PopupClassifier (androidTest).
object PopupWire {
    fun buttonId(shortId: String, label: String): String =
        shortId.takeIf { it.isNotEmpty() } ?: label.lowercase()

    fun buttonRole(id: String, label: String): String = when {
        id.contains("cancel", ignoreCase = true) -> "cancel"
        id.contains("destruct", ignoreCase = true) ||
            label.equals("Delete", ignoreCase = true) -> "destructive"
        else -> "default"
    }

    // Known wire mismatch, kept byte-identical during extraction: the
    // `dangerous` is the key both the Rust SystemPopupButton struct and
    // the iOS runner use; this emitted `destructive` for as long as the
    // route existed, so the flag was silently dropped by every client.
    fun buttonEntry(id: String, label: String, role: String): JSONObject = JSONObject()
        .put("id", id)
        .put("label", label)
        .put("role", role)
        .put("dangerous", role == "destructive")

    fun popupEntry(
        id: String,
        source: String,
        title: String?,
        body: String?,
        buttons: JSONArray,
    ): JSONObject = JSONObject()
        .put("id", id)
        .put("type", "alert")
        .put("source", source)
        .put("title", title ?: "")
        .put("body", body ?: "")
        .put("buttons", buttons)
}

/// Pure JSON assembly for /tree A11yNode payloads. The
/// AccessibilityNodeInfo walking stays in TreeBuilder (androidTest).
object TreeWire {
    /// Strip "<pkg>:id/" prefix so consumers match short ids literally.
    fun shortResourceId(vid: String): String = vid.substringAfter(":id/", vid)

    /// Map android widget class → smix Role camelCase string. Returns
    /// null when no clean mapping (caller omits the role field). Matches
    /// the curated Role enum in smix-screen.
    fun deriveRole(cls: String): String? {
        val tail = cls.substringAfterLast('.')
        return when {
            // Specific *Button subclasses must precede the endsWith
            // catch-all: with the order reversed, RadioButton (and
            // Toggle/CompoundButton) matched "button" and the radio
            // branch was dead — Selector::Role(radio) never matched on
            // Android.
            tail == "RadioButton" -> "radio"
            tail == "ToggleButton" || tail == "CompoundButton" -> "switch"
            tail == "Button" || tail == "ImageButton" || tail.endsWith("Button") -> "button"
            tail == "EditText" || tail.endsWith("EditText") -> "textField"
            tail == "ImageView" -> "image"
            tail == "Switch" || tail == "SwitchCompat" -> "switch"
            tail == "CheckBox" -> "checkBox"
            tail == "TextView" || tail.endsWith("TextView") -> "staticText"
            tail.contains("RecyclerView") || tail.contains("ListView") -> "scrollView"
            tail.contains("TabLayout") || tail == "TabHost" -> "tabBar"
            tail.contains("Toolbar") || tail.contains("ActionBar") -> "navigationBar"
            tail == "WebView" -> "webView"
            else -> null
        }
    }

    fun nodeJson(
        rawType: String,
        identifier: String?,
        label: String?,
        text: String?,
        x: Int,
        y: Int,
        w: Int,
        h: Int,
        enabled: Boolean,
        selected: Boolean,
        hasFocus: Boolean,
        visible: Boolean,
        children: JSONArray,
    ): JSONObject {
        val obj = JSONObject()
        obj.put("rawType", rawType)
        deriveRole(rawType)?.let { obj.put("role", it) }
        identifier?.takeIf { it.isNotEmpty() }?.let { obj.put("identifier", shortResourceId(it)) }
        label?.takeIf { it.isNotEmpty() }?.let { obj.put("label", it) }
        text?.takeIf { it.isNotEmpty() }?.let { obj.put("text", it) }
        obj.put(
            "bounds",
            JSONObject()
                .put("x", x)
                .put("y", y)
                .put("w", w.coerceAtLeast(0))
                .put("h", h.coerceAtLeast(0)),
        )
        obj.put("enabled", enabled)
        obj.put("selected", selected)
        obj.put("hasFocus", hasFocus)
        obj.put("visible", visible)
        obj.put("children", children)
        return obj
    }

    /// Virtual root merging all attached windows into a single dump.
    // `unreadableWindows` is how many attached windows the walk could
    // not read a root out of. They used to be skipped in silence, and a
    // window missing from the tree looks exactly like an app with no
    // accessibility nodes — which is what a consumer concluded, after
    // several rounds of driving by pixel because of it.
    fun windowRootJson(
        maxW: Int,
        maxH: Int,
        children: JSONArray,
        unreadableWindows: Int = 0,
    ): JSONObject = JSONObject()
        .put("rawType", "android.view.WindowRoot")
        .put("bounds", JSONObject().put("x", 0).put("y", 0).put("w", maxW).put("h", maxH))
        .put("enabled", true)
        .put("selected", false)
        .put("hasFocus", false)
        .put("visible", true)
        .put("unreadableWindows", unreadableWindows)
        .put("children", children)

    /// One line per attached window, for answering "why is the app not
    /// in the tree" without reading the runner's source.
    ///
    /// Everything here comes from `AccessibilityWindowInfo` itself: the
    /// type, whether it is the active/focused one, and whether a root
    /// node could be retrieved. A window that is present but unreadable
    /// and a window that is not attached at all are different problems
    /// with the same symptom, and nothing else distinguishes them.
    fun windowsJson(rows: JSONArray): JSONObject = JSONObject()
        .put("status", "ok")
        .put("count", rows.length())
        .put("windows", rows)

    fun windowRowJson(
        index: Int,
        type: Int,
        layer: Int,
        active: Boolean,
        focused: Boolean,
        rootReadable: Boolean,
        packageName: String?,
    ): JSONObject = JSONObject()
        .put("index", index)
        .put("type", type)
        .put("layer", layer)
        .put("active", active)
        .put("focused", focused)
        .put("rootReadable", rootReadable)
        .put("package", packageName ?: JSONObject.NULL)
}
