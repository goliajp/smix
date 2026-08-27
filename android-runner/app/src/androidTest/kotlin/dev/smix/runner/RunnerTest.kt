// smix-android-runner entry + HTTP surface (/health, /tree, tap, swipe,
// press-key, popups, etc.).
//
// Launched via:
//   adb shell am instrument -w -e debug false \
//     -e class dev.smix.runner.RunnerTest \
//     dev.smix.runner.test/androidx.test.runner.AndroidJUnitRunner
//
// /health   — minimal alive probe
// /tree     — UiAutomator2 window walk → A11yNode JSON tree
// (see the SmixHttpServer.serve dispatch table for the full route list)

package dev.smix.runner

import android.app.UiAutomation
import android.graphics.Rect
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityWindowInfo
import com.google.mlkit.vision.common.InputImage
import com.google.mlkit.vision.text.TextRecognition
import com.google.mlkit.vision.text.latin.TextRecognizerOptions
import java.util.concurrent.CountDownLatch as JCountDownLatch
import java.util.concurrent.TimeUnit
import androidx.test.ext.junit.runners.AndroidJUnit4
import android.net.Uri
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.Configurator
import androidx.test.uiautomator.UiDevice
import fi.iki.elonen.NanoHTTPD
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Test
import org.junit.runner.RunWith
import org.xmlpull.v1.XmlPullParser
import org.xmlpull.v1.XmlPullParserFactory
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.util.concurrent.CountDownLatch

@RunWith(AndroidJUnit4::class)
class RunnerTest {
    @Test
    fun runServerForever() {
        val inst = InstrumentationRegistry.getInstrumentation()
        val device = UiDevice.getInstance(inst)
        // Without FLAG_RETRIEVE_INTERACTIVE_WINDOWS,
        // UiAutomation.getWindows() returns only the instrumentation
        // window (status_bar / nav_bar) and filters out the foreground
        // SUT window, so /tree dumps come back empty of app content.
        // Setting this flag at AccessibilityServiceInfo level lets us
        // walk all interactive windows including the SUT.
        //
        // FLAG_REPORT_VIEW_IDS is required for
        // findAccessibilityNodeInfosByViewId to match by resource-id
        // exactly. Without it, viewIdResourceName surfaces on individual
        // nodes (tree dump still gets ids) but the lookup-by-id API
        // silently returns empty, forcing every tap to fall through to
        // the touch path — which fires onClick reliably for top-level
        // Buttons but loses Compose's per-recompose registration race
        // for chained NavHost transitions.
        val info = inst.uiAutomation.serviceInfo
        info.flags = info.flags or
            android.accessibilityservice.AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS or
            android.accessibilityservice.AccessibilityServiceInfo.FLAG_REPORT_VIEW_IDS
        // The recorder (v2.10-C2) listens on the AccessibilityEvent stream; open
        // the two event types it maps so the listener is delivered them. Whether
        // this needs widening further, and whether it perturbs waitForIdle, is
        // proven on the emulator in the C2-S3 e2e (source-only cannot decide it).
        info.eventTypes = info.eventTypes or
            android.view.accessibility.AccessibilityEvent.TYPE_VIEW_CLICKED or
            android.view.accessibility.AccessibilityEvent.TYPE_VIEW_TEXT_CHANGED
        inst.uiAutomation.serviceInfo = info
        // Feed captured events to RecordBuffer while a recording is active. The
        // buffer drops them until /record/start, so this is cheap when idle.
        inst.uiAutomation.setOnAccessibilityEventListener { ev ->
            RecordBuffer.append(
                CapturedAxEvent(
                    type = ev.eventType,
                    viewId = ev.source?.viewIdResourceName,
                    text = ev.text?.joinToString(""),
                    beforeText = ev.beforeText?.toString(),
                    eventTimeMs = ev.eventTime,
                ),
            )
        }
        val server = SmixHttpServer(PORT, device, inst)
        server.start(NanoHTTPD.SOCKET_READ_TIMEOUT, /* daemon = */ false)
        CountDownLatch(1).await()
    }

    companion object {
        const val PORT = 28080
    }
}

class SmixHttpServer(
    port: Int,
    private val device: UiDevice,
    private val instrumentation: android.app.Instrumentation,
) : NanoHTTPD(port) {
    // How many deletes the fallback clear sends when no focused
    // editable node answers. A bound, not a guarantee: a longer field
    // survives it, which is why `/clear-text` reports which path ran.
    private val FALLBACK_DELETE_COUNT = 64

    // /tree serve counter for the X-Tree-Snapshot-Refresh-Count
    // header (parity with the iOS runner).
    private val treeServeCounter = java.util.concurrent.atomic.AtomicLong(0)

    // Rate-limit screenshots (parity with the iOS pacer). A tight OCR
    // loop otherwise contends with UiAutomator; the floor is cheap
    // insurance. Elapsed-realtime clock so it is monotonic across the
    // process. See ScreenshotPacer.
    private val screenshotPacer = ScreenshotPacer(nowMs = { android.os.SystemClock.elapsedRealtime() })

    /**
     * Take a screenshot through the pacer: wait out the interval floor,
     * shoot, and record the wall time (and any failure, which opens the
     * pacer's circuit). Returns null on a failed capture, same as the
     * raw call. A backpressure decision still waits here rather than
     * erroring, so the caller's contract is unchanged.
     */
    private fun pacedScreenshot(): android.graphics.Bitmap? {
        when (val d = screenshotPacer.computeWait()) {
            is PacerDecision.Wait -> if (d.ms > 0) Thread.sleep(d.ms)
            is PacerDecision.Backpressure -> Thread.sleep(d.retryAfterMs)
        }
        val start = android.os.SystemClock.elapsedRealtime()
        val bmp = instrumentation.uiAutomation.takeScreenshot()
        screenshotPacer.record(
            android.os.SystemClock.elapsedRealtime() - start,
            failed = bmp == null,
        )
        return bmp
    }

    /// How long `input-text` waits for a field to take focus.
    ///
    /// The tap that focuses it lands immediately before, and on a cold
    /// screen the IME needs a moment. Two seconds is well past what that
    /// takes on emulator-5554 and still short enough that a genuinely
    /// unfocused field fails while the caller is watching.
    private val FOCUS_SETTLE_MS = 2000L

    /// How long to keep watching for the characters to appear.
    ///
    /// The field publishes to its accessibility node asynchronously, so
    /// this is a deadline on an event that has usually already
    /// happened, not a settle everyone pays. A fill that lands returns
    /// as soon as the reading matches — typically on the first look.
    private val TEXT_LAND_MS = 2000L

    /// How long to wait for the IME window to leave after back.
    private val KEYBOARD_GONE_MS = 2000L

    // NanoHTTPD serves each connection on its own thread, so the body
    // drained in `serve` reaches that request's handler and no other.
    private val drainedBody = ThreadLocal<String>()

    override fun serve(session: IHTTPSession): Response {
        val uri = session.uri
        // Drain the request body before routing, always.
        //
        // NanoHTTPD keeps the connection alive and does NOT consume an
        // unread body, so the leftover bytes become the front of the next
        // request line on the same socket. Four handlers took no session
        // and so never read theirs (/back, /hide-keyboard,
        // /session/close-all, /session/list) — every one of them is
        // POSTed with `{}`, and the request that followed died with
        // `HTTP verb {}GET unhandled`. Found by running a flow on an
        // emulator; no unit test can see it, because the defect is in
        // the connection's state rather than in any one message.
        //
        // Draining here rather than in each handler makes it structural:
        // a new POST route cannot forget.
        drainedBody.remove()
        if (session.method == Method.POST || session.method == Method.PUT) {
            val files = HashMap<String, String>()
            try {
                session.parseBody(files)
                drainedBody.set(files["postData"] ?: "")
            } catch (_: Throwable) {
                // A malformed body is the route's problem to report, not
                // a reason to fail before dispatch. It is consumed either
                // way, which is all this block is for.
                drainedBody.set("")
            }
        }
        return try {
            when {
                uri == "/health" && session.method == Method.GET -> serveHealth()
                uri == "/record/start" && session.method == Method.POST -> serveRecordStart()
                uri == "/record/poll" && session.method == Method.GET -> serveRecordPoll()
                uri == "/record/stop" && session.method == Method.POST -> serveRecordStop()
                uri == "/tree" && session.method == Method.GET -> serveTree()
                uri == "/probe" && session.method == Method.GET -> serveProbe(session)
                uri == "/tap-at-norm-coord" && session.method == Method.POST ->
                    serveTapAtNormCoord(session)
                uri == "/swipe-at-norm-coord" && session.method == Method.POST ->
                    serveSwipeAtNormCoord(session)
                uri == "/swipe-once" && session.method == Method.POST ->
                    serveSwipeOnce(session)
                uri == "/press-key" && session.method == Method.POST ->
                    servePressKey(session)
                uri == "/back" && session.method == Method.POST -> serveBack()
                uri == "/hide-keyboard" && session.method == Method.POST -> serveHideKeyboard()
                uri == "/set-orientation" && session.method == Method.POST ->
                    serveSetOrientation(session)
                uri == "/tap-by-id" && session.method == Method.POST -> serveTapById(session)
                uri == "/double-tap-at-norm-coord" && session.method == Method.POST ->
                    serveDoubleTapAtNormCoord(session)
                uri == "/long-press-at-norm-coord" && session.method == Method.POST ->
                    serveLongPressAtNormCoord(session)
                uri == "/input-text" && session.method == Method.POST -> serveInputText(session)
                uri == "/clear-text" && session.method == Method.POST -> serveClearText(session)
                uri == "/windows" && session.method == Method.GET -> serveWindows()
                uri == "/foreground" && session.method == Method.POST -> serveForeground(session)
                uri == "/find-text-by-ocr" && session.method == Method.POST ->
                    serveFindTextByOcr(session)
                uri == "/system-popups" && session.method == Method.GET -> serveSystemPopups()
                uri == "/system-popup-action" && session.method == Method.POST ->
                    serveSystemPopupAction(session)
                uri == "/webview-eval" && session.method == Method.POST ->
                    serveWebViewEvalProxy(session)
                uri == "/session/open" && session.method == Method.POST ->
                    serveSessionOpen(session)
                uri == "/session/close" && session.method == Method.POST ->
                    serveSessionClose(session)
                uri == "/session/close-all" && session.method == Method.POST ->
                    serveSessionCloseAll()
                uri == "/session/list" && session.method == Method.POST ->
                    serveSessionList()
                uri == "/session/renew-activation" && session.method == Method.POST ->
                    serveSessionRenewActivation(session)
                uri == "/session/launch-app" && session.method == Method.POST ->
                    serveSessionAppLifecycle(session, terminate = false)
                uri == "/session/terminate-app" && session.method == Method.POST ->
                    serveSessionAppLifecycle(session, terminate = true)
                uri == "/session/relaunch-app" && session.method == Method.POST ->
                    serveSessionRelaunchApp(session)
                else -> serveNotImplemented(uri, session.method.name)
            }
        } catch (e: Throwable) {
            val body = RunnerWire.internalErrorBody(
                e.message ?: e.javaClass.simpleName,
                e.javaClass.name,
            )
            newFixedLengthResponse(Response.Status.INTERNAL_ERROR, "application/json", body)
        }
    }

    private fun serveHealth(): Response {
        val body = RunnerWire.healthBody(SmixRunner.VERSION)
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    // ---- recorder (v2.10-C2) ------------------------------------------
    // /record/start begins capture; /record/poll drains accumulated IRAction
    // JSON (streaming); /record/stop drains the remainder and deactivates.

    private fun serveRecordStart(): Response {
        RecordBuffer.start()
        return newFixedLengthResponse(Response.Status.OK, "application/json", "{\"ok\":true}")
    }

    private fun serveRecordPoll(): Response = recordEventsResponse(RecordBuffer.poll())

    private fun serveRecordStop(): Response = recordEventsResponse(RecordBuffer.stop())

    private fun recordEventsResponse(actions: List<String>): Response {
        // Each action is already an IRAction JSON object string.
        val body = "{\"events\":[" + actions.joinToString(",") + "]}"
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    /**
     * What the in-process probe says, or that there is not one.
     *
     * The probe lives in the app under test, not here — it can read the
     * semantics tree because it is in that process, and this runner cannot.
     * So this asks across a content provider and reports the answer
     * verbatim, including its absence.
     *
     * Absence is an answer, not an error: most apps will not carry the
     * probe and everything works without it. What must never happen is the
     * two being confused. "The screen is busy" and "nobody told me
     * anything" want different waits, and one value for both is how 250 ms
     * of guessing became indistinguishable from being told.
     *
     * The app is named by the caller. This runner has no notion of a
     * current app — sessions are per-id and the target arrives per request
     * — so inventing one here would answer about whichever app it guessed.
     */
    private fun serveProbe(session: NanoHTTPD.IHTTPSession): Response {
        val app = session.parameters["app"]?.firstOrNull().orEmpty()
        if (app.isEmpty()) {
            return newFixedLengthResponse(
                Response.Status.OK, "application/json",
                """{"present":false,"why":"no app was named; ask /probe?app=<applicationId>"}""",
            )
        }
        val uri = Uri.parse("content://$app.smixprobe")
        val body = try {
            val ctx = InstrumentationRegistry.getInstrumentation().context
            val hello = ctx.contentResolver.call(uri, "hello", null, null)
            if (hello == null) {
                """{"present":false,"why":"$app has no smix probe in this build"}"""
            } else {
                // Milliseconds the semantics tree has looked unchanged, or
                // -1 for "never looked". Not a boolean: the caller decides
                // how still is still enough, and a boolean would bake that
                // threshold in here where nobody can see it.
                val quiet = ctx.contentResolver.call(uri, "idle", null, null)
                    ?.getLong("quietMs") ?: -1L
                val roots = hello.getInt("roots", 0)
                val version = hello.getString("version") ?: "?"
                """{"present":true,"version":"$version","roots":$roots,"quietMs":$quiet}"""
            }
        } catch (e: IllegalArgumentException) {
            // Resolving an authority nothing declares throws rather than
            // returning null, and this is the ORDINARY case: almost no app
            // carries the probe. Reading it back as an exception would make
            // the normal state look like a fault, which is how a caller
            // learns to ignore the field.
            """{"present":false,"why":"$app declares no smix probe — it is not in this build"}"""
        } catch (e: SecurityException) {
            // There and refusing. A different problem from not being there,
            // and the caller can act on the difference: the probe answers
            // adb, the host app, and smix's own instrumentation.
            """{"present":false,"why":"$app has a smix probe and it refused this caller"}"""
        } catch (e: Exception) {
            """{"present":false,"why":"asking $app.smixprobe raised ${e.javaClass.simpleName}"}"""
        }
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveTree(): Response {
        // Walk UiAutomation.windows directly instead of
        // device.dumpWindowHierarchy. The XML dump only emits one window
        // hierarchy at a time and on some Android versions skips the
        // foreground app window (returning only status_bar — observed on
        // API 33 emulator with android-runner test instrumentation in
        // separate process from the app). Direct AccessibilityNodeInfo
        // walk via UiAutomation iterates all attached windows and
        // surfaces Compose testTag via
        // AccessibilityNodeInfo.viewIdResourceName.
        //
        // Snapshot-freshness headers, parity with the iOS runner:
        // `X-Tree-Snapshot-Refresh-Count` is a monotonic count of
        // successful /tree serves since runner boot,
        // `X-Tree-Snapshot-Wall-Ms` is this walk's end-to-end wall.
        // The pair lets callers detect snapshot pipeline stalls /
        // slowdown across a batch without any wire-body change.
        val walkStart = System.currentTimeMillis()
        val automation = instrumentation.uiAutomation
        val root = TreeBuilder.fromWindows(automation)
        val wallMs = System.currentTimeMillis() - walkStart
        val refreshCount = treeServeCounter.incrementAndGet()
        val resp = newFixedLengthResponse(
            Response.Status.OK,
            "application/json",
            root.toString(),
        )
        resp.addHeader("X-Tree-Snapshot-Refresh-Count", refreshCount.toString())
        resp.addHeader("X-Tree-Snapshot-Wall-Ms", wallMs.toString())
        return resp
    }

    /// Every attached window, and whether its root could be read.
    ///
    /// `/tree` answers "what is on screen"; this answers "why is
    /// something not". A consumer reported the tree carrying only the
    /// SystemUI windows and not their app's, and neither of us could
    /// tell from here whether their app's window was absent from the
    /// list or present and unreadable — different problems, identical
    /// symptom, and no way to look.
    private fun serveWindows(): Response {
        val rows = JSONArray()
        val windows = instrumentation.uiAutomation.windows
        for ((i, w) in windows.withIndex()) {
            val root = w.root
            rows.put(
                TreeWire.windowRowJson(
                    index = i,
                    type = w.type,
                    layer = w.layer,
                    active = w.isActive,
                    focused = w.isFocused,
                    rootReadable = root != null,
                    packageName = root?.packageName?.toString(),
                ),
            )
            root?.recycle()
        }
        val body = TreeWire.windowsJson(rows).toString()
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveTapAtNormCoord(session: IHTTPSession): Response {
        val req = RunnerWire.decodeNormCoord(readBodyString(session))
        val w = device.displayWidth
        val h = device.displayHeight
        val px = RunnerWire.normToPixel(req.nx, w)
        val py = RunnerWire.normToPixel(req.ny, h)
        val ok = device.click(px, py)
        // Give the dispatched IO event time to render before /tree probes
        // observe the post-tap UI state.
        device.waitForIdle(500)
        val body = RunnerWire.tapAtNormCoordBody(ok, w, h, px, py)
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveSwipeAtNormCoord(session: IHTTPSession): Response {
        val req = RunnerWire.decodeSwipeAtNormCoord(readBodyString(session))
        val w = device.displayWidth
        val h = device.displayHeight
        val q = RunnerWire.SwipeQuad(
            RunnerWire.normToPixel(req.fromNx, w),
            RunnerWire.normToPixel(req.fromNy, h),
            RunnerWire.normToPixel(req.toNx, w),
            RunnerWire.normToPixel(req.toNy, h),
        )
        // 50 steps ≈ 500ms — enough to register as fling/scroll, not so
        // fast it's interpreted as a tap.
        val ok = device.swipe(q.x1, q.y1, q.x2, q.y2, 50)
        device.waitForIdle(500)
        val body = RunnerWire.swipeAtNormCoordBody(ok, q)
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveSwipeOnce(session: IHTTPSession): Response {
        val direction = RunnerWire.decodeSwipeOnce(readBodyString(session))
        val q = RunnerWire.swipeOnceCoords(direction, device.displayWidth, device.displayHeight)
            ?: return errorJson(
                Response.Status.BAD_REQUEST,
                "bad_direction",
                "expected one of up/down/left/right, got '$direction'",
            )
        val ok = device.swipe(q.x1, q.y1, q.x2, q.y2, 50)
        device.waitForIdle(500)
        val body = RunnerWire.swipeOnceBody(ok, direction, q)
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun servePressKey(session: IHTTPSession): Response {
        val key = RunnerWire.decodePressKey(readBodyString(session))
        val code = KeyMap.androidKeyCode(key) ?: return errorJson(
            Response.Status.BAD_REQUEST,
            "unknown_key",
            "no Android KeyEvent mapping for '$key'",
        )
        device.pressKeyCode(code)
        device.waitForIdle(500)
        val body = RunnerWire.pressKeyBody(key, code)
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveBack(): Response {
        val ok = device.pressBack()
        device.waitForIdle(500)
        val body = RunnerWire.backBody(ok)
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveHideKeyboard(): Response {
        // There is no first-class UiAutomator2 hide-keyboard, and the
        // closest thing is back. Back closes a keyboard when one is up
        // and closes whatever is in front when none is — so the comment
        // that used to sit here, claiming "no harm if no keyboard is
        // up", described an intent the code did not carry out. Measured
        // on emulator-5554: with nothing focused this answered ok:true
        // and sent the fixture app to the launcher.
        //
        // That matters beyond the surprise. iOS needs the dismissal (the
        // keyboard covers the button the next step taps) and maestro
        // treats it as a no-op when there is no keyboard, so a flow
        // written once for both platforms cannot be asked to know
        // whether a keyboard is up before saying "put it away". The
        // no-op is what makes the verb portable.
        //
        // An input-method window is the evidence, and it is the same
        // window list /windows already serves.
        if (!keyboardIsUp()) {
            val body = RunnerWire.hideKeyboardBody(true)
            return newFixedLengthResponse(Response.Status.OK, "application/json", body)
        }
        // `pressBack()` returns whether the key event was injected, and
        // a consumer measured it answering false while the keyboard
        // came down — `{"ok":false}` with the IME gone from the window
        // list one call later. Its boolean is about the injection, not
        // about the outcome, and this route was reporting the wrong one
        // of those.
        //
        // The outcome has its own evidence, and it is the same evidence
        // the no-op decision above already uses. Ask it afterwards.
        device.pressBack()
        device.waitForIdle(500)
        val gone = awaitKeyboardGone(KEYBOARD_GONE_MS)
        val body = RunnerWire.hideKeyboardBody(gone)
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    /// Wait for the input-method window to leave, up to a budget.
    ///
    /// The dismissal is animated, so the window outlives the key press
    /// by a frame or two; a single look straight afterwards can catch
    /// it still there and call a working dismissal a failure. Same
    /// shape as the text read-back next door — one instant cannot
    /// answer a question about a transition.
    private fun awaitKeyboardGone(budgetMs: Long): Boolean {
        val deadline = android.os.SystemClock.elapsedRealtime() + budgetMs
        while (true) {
            if (!keyboardIsUp()) return true
            if (android.os.SystemClock.elapsedRealtime() >= deadline) return false
            Thread.sleep(50)
        }
    }

    /// Is an input-method window on screen?
    ///
    /// `TYPE_INPUT_METHOD` is what the IME's own window reports, so this
    /// asks the same source /windows does rather than guessing from
    /// focus — a focused text field with the keyboard dismissed is an
    /// ordinary state, and it is exactly the one that used to send back
    /// through to the app.
    private fun keyboardIsUp(): Boolean =
        instrumentation.uiAutomation.windows.any {
            it.type == AccessibilityWindowInfo.TYPE_INPUT_METHOD
        }

    private fun serveSetOrientation(session: IHTTPSession): Response {
        val orientation = RunnerWire.decodeSetOrientation(readBodyString(session))
        when (orientation) {
            "portrait" -> device.setOrientationNatural()
            "landscapeLeft" -> device.setOrientationLeft()
            "landscapeRight" -> device.setOrientationRight()
            "portraitUpsideDown" ->
                // UiDevice has no direct "upside-down portrait"; emulate
                // by rotating 180° from natural (best-effort, may vary
                // by device).
                device.setOrientationNatural().also { device.setOrientationLeft(); device.setOrientationLeft() }
            else -> return errorJson(
                Response.Status.BAD_REQUEST,
                "bad_orientation",
                "expected portrait | landscapeLeft | landscapeRight | portraitUpsideDown, got '$orientation'",
            )
        }
        device.waitForIdle(800)
        val body = RunnerWire.setOrientationBody(orientation)
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveTapById(session: IHTTPSession): Response {
        val id = RunnerWire.decodeTapById(readBodyString(session))
        // NanoHTTPD lowercases header names.
        val targetPackage = session.headers["app-bundle-id"]
        // Prefer AccessibilityNodeInfo.performAction(ACTION_CLICK) over
        // UiObject2.click() because Compose Button onClick fires
        // reliably through the a11y action path even when a sibling
        // AndroidView (MapView / PlayerView / PreviewView) GestureDetector
        // would intercept a synthesized touch event. Fall back to
        // UiObject2.click() for non-Compose Views without ACTION_CLICK
        // in their actions list.
        //
        // Poll up to 1500ms for both findNodeByViewId AND ACTION_CLICK
        // presence. Compose recompose vs. a11y semantics commit is
        // asynchronous (NavHost transitions especially); a single-shot
        // probe loses the race intermittently. Polling makes /tap-by-id
        // auto-await the a11y tree settling without leaking timing
        // details to the caller. Latency cost for present targets is
        // small (first iter hits in <100ms); for genuinely missing
        // targets it adds up to 1500ms (still correct, just slower error).
        val pollDeadlineMs = System.currentTimeMillis() + 1500
        var clicked = false
        var path = "none"
        var sawNode = false
        var sawActionClick = false
        var matchedSpelling: String? = null
        var sawStrictHit = false
        while (System.currentTimeMillis() < pollDeadlineMs && !clicked) {
            val (targetNode, matched) = findNodeByViewId(id, targetPackage)
            if (matched != null) {
                matchedSpelling = matched
                sawStrictHit = true
            }
            if (targetNode != null) {
                sawNode = true
                val actions = targetNode.actionList
                val hasClickAction =
                    actions.any { it.id == AccessibilityNodeInfo.ACTION_CLICK }
                if (hasClickAction) {
                    sawActionClick = true
                    clicked = targetNode.performAction(AccessibilityNodeInfo.ACTION_CLICK)
                    if (clicked) path = "a11y"
                }
                targetNode.recycle()
                if (clicked) break
            }
            if (!clicked) Thread.sleep(75)
        }
        // Touch fallback only after a11y poll fully exhausted — `path:"touch"`
        // therefore means "node never grew an ACTION_CLICK within 1.5s, or
        // never appeared in the a11y tree at all".
        if (!clicked) {
            val candidates = listOf(
                By.res(Regex(".*:id/${Regex.escape(id)}$").toPattern()),
                By.res(id),
            )
            for (sel in candidates) {
                val obj = device.findObject(sel)
                if (obj != null) {
                    obj.click()
                    clicked = true
                    path = "touch"
                    break
                }
            }
        }
        device.waitForIdle(500)
        val body = RunnerWire.tapByIdBody(clicked, id, path, sawNode, sawActionClick)
        val response = newFixedLengthResponse(Response.Status.OK, "application/json", body)
        // A header, not a body field. v1.0.23 settled the same question
        // for the tree-snapshot counters: the body of this route has a
        // Rust-side parser reading its five fields, so adding one is a
        // wire-contract change while adding a header is not.
        //
        // "miss" is distinct from "walk": the walk answering is a
        // different fact from nothing answering at all.
        val kind = if (!sawNode && !sawStrictHit) {
            "miss"
        } else {
            RunnerWire.viewIdMatchKind(matchedSpelling, id, targetPackage)
        }
        response.addHeader("X-View-Id-Match", kind)
        return response
    }

    /// Find first node matching `viewIdResourceName`, returning it
    /// alongside the spelling that hit (null when the manual walk
    /// answered instead). Candidates come from
    /// `RunnerWire.viewIdCandidates`: the bare id, the app under test
    /// qualified by the App-Bundle-Id header, then the runner's own
    /// test package. This comment used to name `com.example.app` among
    /// them — that placeholder was removed from the candidate list, and
    /// saying otherwise here outlived the code by a day.
    /// Caller owns the returned node and MUST
    /// recycle.
    ///
    /// Manual AccessibilityNodeInfo recycling during tree traversal
    /// poisons the framework cache and causes subsequent /tree queries
    /// to return stale state, breaking yaml flows where an optional
    /// tap (find-fail) precedes a real tap. Strict
    /// findAccessibilityNodeInfosByViewId handles recycling internally
    /// — let the framework own that lifecycle.
    ///
    /// We walk `uiAutomation.windows` (multi-window) instead of
    /// `rootInActiveWindow` (single-window). Mirrors `/tree` builder and
    /// UiObject2 lookup, which both walk all windows. Compose app
    /// content isn't always the focused window (e.g. when a SystemUI
    /// IME / overlay grabs focus); single-window lookup silently misses
    /// the app content and forces touch-fallback for every tap. Multi-
    /// window matches what the caller sees in /tree, which is the
    /// consistent surface they expect to address by id.
    private fun findNodeByViewId(
        shortId: String,
        targetPackage: String?,
    ): Pair<AccessibilityNodeInfo?, String?> {
        val candidates = RunnerWire.viewIdCandidates(shortId, targetPackage)
        for (window in instrumentation.uiAutomation.windows) {
            val root = window.root ?: continue
            for (cand in candidates) {
                val hits = root.findAccessibilityNodeInfosByViewId(cand)
                if (hits.isNotEmpty()) {
                    val node = hits[0]
                    hits.drop(1).forEach { it.recycle() }
                    // The spelling travels with the node. Without it the
                    // caller cannot tell a strict hit from the walk
                    // below, and those two returning the same node is
                    // exactly how a placeholder package survived here.
                    return node to cand
                }
            }
        }
        // Manual walk fallback. Reality observed: Compose
        // `testTagsAsResourceId = true` emits viewIdResourceName on
        // every Composable Button and /tree dumps surface the ids
        // correctly, but `findAccessibilityNodeInfosByViewId` returns
        // empty for them (likely because Compose synthesizes virtual
        // resource ids that aren't registered with the framework's
        // resource lookup table — a structural Compose ↔
        // AccessibilityFramework gap; not fixable by FLAG_REPORT_VIEW_IDS
        // or window iteration alone).
        //
        // The walk doesn't recycle traversed children (manual recycle
        // during tree traversal evicts framework cache entries that
        // subsequent ops need, breaking optional-tap patterns). Memory
        // cost per query is small — one walk per /tap-by-id — and the
        // leak only matters under sustained pressure. Accept it for
        // chained NavHost reliability.
        //
        // Flush pending accessibility events before re-reading windows.
        // UiAutomator2 caches windows[].root snapshots; after a NavHost
        // transition the visual UI advances (screenshots confirm the
        // next screen is rendered) but `automation.windows[].root` still
        // reports the previous screen unless we wait for the
        // accessibility event stream to settle. `waitForIdle(idleTime,
        // globalTimeout)` blocks until no accessibility events arrived
        // for `idleTime` ms or `globalTimeout` elapses. 500ms idle /
        // 2000ms cap is generous for Compose recompose + NavHost
        // transition without leaking into pathological slow runs.
        instrumentation.uiAutomation.waitForIdle(500, 2000)
        for (window in instrumentation.uiAutomation.windows) {
            val root = window.root ?: continue
            root.refresh()
            val found = walkFindByViewIdNoRecycle(root, shortId)
            if (found != null) return found to null
        }
        return null to null
    }

    /// Recursive walk matching `viewIdResourceName` short suffix
    /// (post-`:id/` strip). Does NOT recycle child nodes — see the
    /// findNodeByViewId comment for the cache-poisoning constraint that
    /// forces this. Returns a fresh `obtain()` copy on match so the
    /// caller's `.recycle()` is independent of the framework's window-
    /// owned tree.
    private fun walkFindByViewIdNoRecycle(
        node: AccessibilityNodeInfo,
        shortId: String,
    ): AccessibilityNodeInfo? {
        // Per-node refresh re-syncs each AccessibilityNodeInfo with
        // current framework state. Coarse root.refresh() (caller side)
        // only refreshes the top node; cached descendants keep stale
        // state for chained NavHost transitions. Refreshing each
        // descendant before reading viewIdResourceName forces the
        // framework to honor any pending semantics updates from the most
        // recent Compose recompose.
        node.refresh()
        val vid = node.viewIdResourceName
        if (vid != null && vid.isNotEmpty()) {
            val short = vid.substringAfter(":id/", vid)
            if (short == shortId) {
                return AccessibilityNodeInfo.obtain(node)
            }
        }
        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            val found = walkFindByViewIdNoRecycle(child, shortId)
            if (found != null) return found
            // Intentional: no `child.recycle()` — see comment above.
        }
        return null
    }

    private fun serveDoubleTapAtNormCoord(session: IHTTPSession): Response {
        val req = RunnerWire.decodeNormCoord(readBodyString(session))
        val px = RunnerWire.normToPixel(req.nx, device.displayWidth)
        val py = RunnerWire.normToPixel(req.ny, device.displayHeight)
        device.click(px, py)
        // Standard double-tap inter-tap window — 150ms is below most
        // systems' DOUBLE_TAP_TIMEOUT (300ms) so events register as a
        // pair, not as two separate taps.
        Thread.sleep(150)
        device.click(px, py)
        device.waitForIdle(500)
        val body = RunnerWire.doubleTapBody(px, py)
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveLongPressAtNormCoord(session: IHTTPSession): Response {
        val req = RunnerWire.decodeLongPressAtNormCoord(readBodyString(session))
        val px = RunnerWire.normToPixel(req.nx, device.displayWidth)
        val py = RunnerWire.normToPixel(req.ny, device.displayHeight)
        device.swipe(px, py, px, py, RunnerWire.longPressSteps(req.durationMs))
        device.waitForIdle(500)
        val body = RunnerWire.longPressBody(px, py, req.durationMs)
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveInputText(session: IHTTPSession): Response {
        val req = RunnerWire.decodeInputText(readBodyString(session))
        val text = req.text
        val focusPx = focusRectPx(req.focusIn)
        // `adb shell input text` types into whichever field is focused,
        // and says nothing about whether one was. Its exit code means
        // "the key events were dispatched", not "the characters landed"
        // — so this used to answer OK unconditionally, and a consumer
        // reported the first named fill on a screen typing nothing while
        // reporting success. Reproduced on emulator-5554 with the
        // fixture: `smix fill` said "17 chars", the field held none.
        //
        // The clear handler next door has always done this properly:
        // find the focused editable node, act on it, report what
        // happened. This is that, for the other half of the same job.
        //
        // Waiting for the focus rather than sampling once: the caller
        // taps to focus immediately before, and an IME that has not
        // finished coming up has no focused editable node yet. That gap
        // is the whole defect — the second fill on a screen always
        // worked because by then the keyboard was already open.
        val focused = awaitEditableFocus(FOCUS_SETTLE_MS, focusPx)
            ?: return errorJson(
                Response.Status.INTERNAL_ERROR,
                "no_focused_field",
                "input-text: no editable field had focus after " +
                    "${FOCUS_SETTLE_MS}ms. `input text` types into the focused " +
                    "field and there was none, so the characters would have " +
                    "gone nowhere while this reported success.",
            )

        val before = focused.text?.toString() ?: ""
        // The node says whether it masks. Nothing here guesses it from
        // the text: `aaaa` is four of the same character and is not a
        // mask, and a guess would quietly stop checking content for
        // anyone whose password repeats one.
        val masked = focused.isPassword
        // Name the field. "The focused field holds 0" was a claim about
        // a node the reader could not identify, and the node it meant
        // was not always the one they had asked for — a walk over every
        // window returns the first focused editable node, and while an
        // IME is up that need not be the app's.
        val whichField = focused.viewIdResourceName
            ?: focused.className?.toString()
            ?: "an unnamed node"
        runShellCommand(RunnerWire.inputTextCommand(text))
        device.waitForIdle(500)

        // Read the field back. `input text` cannot report a miss, so the
        // only honest evidence that the characters arrived is the node
        // saying so.
        val after = awaitLanded(focused, before, text, masked, TEXT_LAND_MS)
        focused.recycle()
        if (!RunnerWire.textLanded(before, after, text, masked)) {
            return errorJson(
                Response.Status.INTERNAL_ERROR,
                "text_did_not_land",
                if (masked) {
                    "input-text: dispatched ${text.length} characters into a " +
                        "masked field and it grew by " +
                        "${after.length - before.length} (from ${before.length} " +
                        "to ${after.length}) after waiting ${TEXT_LAND_MS}ms. " +
                        "The field was $whichField. A " +
                        "masked node reports one character per character held " +
                        "and never the characters, so the length difference is " +
                        "the only evidence there is, and it does not add up."
                } else {
                    "input-text: dispatched ${text.length} characters and the " +
                        "focused field holds ${after.length} " +
                        "(before: ${before.length}) after waiting ${TEXT_LAND_MS}ms. " +
                        "The field was $whichField (masked: $masked). " +
                        "The keys went somewhere other than the field."
                },
            )
        }
        val body = RunnerWire.inputTextBody(text)
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    /// The focused editable node, waited for rather than sampled.
    ///
    /// Returns null when none appears in time. The caller recycles.
    ///
    /// `refresh()` before reading `isEditable`, for the reason
    /// `nodeToJson` already gives: a node handed out by the framework
    /// carries the fields it had when it was fetched, and a Compose
    /// field that has just taken focus is exactly the case where those
    /// are out of date. Without it this reported "no editable field had
    /// focus" for two seconds about a field the tree showed as focused
    /// in the same instant — the two reads disagreed because only one
    /// of them was current.
    /// Convert the caller's tap point to screen pixels, or null when
    /// they did not name a field.
    /// The named element's box in screen pixels, or null when the
    /// caller named no element.
    private fun focusRectPx(r: RunnerWire.NormRect?): IntArray? =
        r?.let {
            val l = RunnerWire.normToPixel(it.nx, device.displayWidth)
            val t = RunnerWire.normToPixel(it.ny, device.displayHeight)
            intArrayOf(
                l,
                t,
                l + RunnerWire.normToPixel(it.nw, device.displayWidth),
                t + RunnerWire.normToPixel(it.nh, device.displayHeight),
            )
        }

    private fun awaitEditableFocus(
        budgetMs: Long,
        focusPx: IntArray? = null,
    ): AccessibilityNodeInfo? {
        val deadline = android.os.SystemClock.elapsedRealtime() + budgetMs
        while (true) {
            focusedTextNode(focusPx)?.let { return it }
            if (android.os.SystemClock.elapsedRealtime() >= deadline) {
                return null
            }
            device.waitForIdle(100)
            Thread.sleep(50)
        }
    }

    /// Whatever holds focus and can be typed into, asked two ways.
    ///
    /// `findFocus(FOCUS_INPUT)` is the framework's answer and it is the
    /// right question to ask first. It is not the only place the answer
    /// lives: Compose keeps focus in its own semantics layer, and on a
    /// freshly composed screen it comes back empty about a field the
    /// tree reports as focused in the same instant — with the IME
    /// window already up. Measured on emulator-5554, twice, on the
    /// first fill after the activity starts. That "first fill on a
    /// screen" is exactly the shape a consumer reported.
    ///
    /// So when the direct query has nothing, walk for it. The walk is
    /// what the tree route already does, and it costs a traversal only
    /// on the path where the cheap answer failed.
    private fun focusedTextNode(focusPx: IntArray?): AccessibilityNodeInfo? {
        val focused = anyFocusedTextNode() ?: return null
        if (focusPx == null || holdsFocusPoint(focused, focusPx)) {
            return focused
        }
        // The focused field is not the one under the tap. That is the
        // defect this point exists for — unless nothing typeable is
        // under the tap at all, which is the ordinary case of naming a
        // container: a consumer's fields carry only a contentDescription
        // on the wrapping layout, so the point is the layout's centre
        // and the field that takes focus sits somewhere inside it.
        // Refusing there broke every fill in that app.
        val aimedAt = editableUnder(focusPx)
        if (aimedAt == null) {
            return focused
        }
        aimedAt.recycle()
        focused.recycle()
        return null
    }

    /// Whatever holds focus and can be typed into, without asking where.
    private fun anyFocusedTextNode(): AccessibilityNodeInfo? {
        val direct = instrumentation.uiAutomation.findFocus(
            AccessibilityNodeInfo.FOCUS_INPUT,
        )
        if (direct != null) {
            direct.refresh()
            if (direct.isEditable || direct.canTakeText()) return direct
            direct.recycle()
        }
        for (window in instrumentation.uiAutomation.windows) {
            val root = window.root ?: continue
            searchFocusedTextNode(root, null)?.let { return it }
        }
        return null
    }

    /// A typeable node whose own bounds hold this point, focused or not.
    ///
    /// The point only decides anything when there is one: if the caller
    /// aimed at a field and a *different* field has focus, the tap has
    /// not landed yet and typing now would go to the wrong place. If
    /// they aimed at something that is not itself typeable, the point
    /// has nothing to say and whatever took focus is the answer.
    private fun editableUnder(focusPx: IntArray): AccessibilityNodeInfo? {
        for (window in instrumentation.uiAutomation.windows) {
            val root = window.root ?: continue
            searchEditableUnder(root, focusPx)?.let { return it }
        }
        return null
    }

    private fun searchEditableUnder(
        node: AccessibilityNodeInfo,
        focusPx: IntArray,
    ): AccessibilityNodeInfo? {
        node.refresh()
        if ((node.isEditable || node.canTakeText()) && holdsFocusPoint(node, focusPx)) {
            return node
        }
        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            val hit = searchEditableUnder(child, focusPx)
            if (hit != null) return hit
            child.recycle()
        }
        return null
    }

    /// Is this the field the caller named?
    ///
    /// Focus in Compose does not move synchronously with the tap that
    /// moves it, so "some editable node has focus" is true before the
    /// tap as well as after — and a fill naming one field was measured
    /// clearing and typing into another. With a tap point the answer is
    /// no until focus reaches the field containing it; without one,
    /// every focused field qualifies, which is what a fill with no
    /// selector means.
    private fun holdsFocusPoint(
        node: AccessibilityNodeInfo,
        focusPx: IntArray?,
    ): Boolean {
        if (focusPx == null) return true
        val r = android.graphics.Rect()
        node.getBoundsInScreen(r)
        return RunnerWire.focusAccepts(r.left, r.top, r.right, r.bottom, focusPx)
    }

    /// Depth-first for a focused node that accepts text.
    private fun searchFocusedTextNode(
        node: AccessibilityNodeInfo,
        focusPx: IntArray?,
    ): AccessibilityNodeInfo? {
        node.refresh()
        if (node.isFocused && (node.isEditable || node.canTakeText()) &&
            holdsFocusPoint(node, focusPx)
        ) {
            return node
        }
        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            val hit = searchFocusedTextNode(child, focusPx)
            if (hit != null) return hit
            child.recycle()
        }
        return null
    }

    /// Does this node accept text, whatever it says about being editable?
    ///
    /// `isEditable` is a claim a toolkit has to make about itself, and
    /// a Compose text field does not always make it. What is not
    /// optional is the action: a node that will take `ACTION_SET_TEXT`
    /// is a node characters can be put into, and that is the thing
    /// being asked about.
    private fun AccessibilityNodeInfo.canTakeText(): Boolean =
        actionList.any { it.id == AccessibilityNodeInfo.ACTION_SET_TEXT }

    /// Current text of whatever holds input focus, read fresh.
    ///
    /// "Fresh" is what `refresh()` buys: without it this returned the
    /// value the node carried when the framework handed it over, which
    /// on a field that had just been typed into is the value from
    /// before the typing. It read 0 characters, every time, about a
    /// field the tree showed holding all seventeen.
    private fun readFocusedText(): String? {
        val node = instrumentation.uiAutomation.findFocus(
            AccessibilityNodeInfo.FOCUS_INPUT,
        ) ?: return null
        node.refresh()
        val text = node.text?.toString()
        node.recycle()
        return text
    }

    /// Wait for the field to be holding `text`, up to a budget.
    ///
    /// One read after a fixed settle is not enough and cannot be made
    /// enough by lengthening the settle: a Compose field publishes to
    /// its accessibility node asynchronously, and a consumer measured
    /// 17, 15 and 0 characters from the same action. Any single instant
    /// can catch it mid-publish, so the question "did the characters
    /// arrive" is answered by watching until they have, or until there
    /// is no more time to wait.
    ///
    /// Returns the last reading either way, so a refusal can say what
    /// it actually saw.
    /// Poll until the field says the characters arrived, by whatever
    /// question that field can answer.
    ///
    /// This used to poll on `contains`, which a masked field can never
    /// satisfy: every masked fill burned the whole budget and then
    /// reported the last thing it read. That is also why the same
    /// defect showed two faces — a slow publication read back as
    /// nothing at all, a settled one as the right number of bullets.
    /// Polling on the verdict itself returns as soon as it is true and
    /// gives the slow case the full budget it was already spending.
    private fun awaitLanded(
        node: AccessibilityNodeInfo,
        before: String,
        dispatched: String,
        masked: Boolean,
        budgetMs: Long,
    ): String {
        val deadline = android.os.SystemClock.elapsedRealtime() + budgetMs
        var last = ""
        while (true) {
            node.refresh()
            last = node.text?.toString() ?: ""
            if (RunnerWire.textLanded(before, last, dispatched, masked)) return last
            if (android.os.SystemClock.elapsedRealtime() >= deadline) return last
            Thread.sleep(50)
        }
    }

    /// Empty the focused field in one request.
    ///
    /// The host used to do this by posting `/press-key DELETE` fifty
    /// times — fifty sequential round trips over the adb forward, on
    /// every fill once fill started clearing first. Worse than slow:
    /// fifty deletes do not empty a field holding more than fifty
    /// characters, so the text that followed landed after the
    /// remainder and the caller was told it had replaced the value.
    ///
    /// `ACTION_SET_TEXT` is exact and length-independent, and it is
    /// what `UiObject2.clear()` uses underneath. When no focused
    /// editable node answers — a field the accessibility tree cannot
    /// address is exactly the case the key-event path exists for —
    /// fall back to delete keys, still in one shell exec, and say
    /// which path ran so the caller knows whether the answer is exact.
    private fun serveClearText(session: IHTTPSession): Response {
        // The clear that precedes a fill needs the same target as the
        // typing. Without it a fill naming one field empties whichever
        // field happened to hold focus — measured on emulator-5554,
        // compose_password went from ten characters to none while the
        // caller had named compose_input.
        val focusPx = focusRectPx(RunnerWire.decodeClearText(readBodyString(session)))
        val focused = awaitEditableFocus(FOCUS_SETTLE_MS, focusPx)
        if (focused != null && focused.isEditable) {
            val args = android.os.Bundle()
            args.putCharSequence(
                android.view.accessibility.AccessibilityNodeInfo
                    .ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,
                "",
            )
            val ok = focused.performAction(
                android.view.accessibility.AccessibilityNodeInfo.ACTION_SET_TEXT,
                args,
            )
            focused.recycle()
            if (ok) {
                device.waitForIdle(500)
                val body = RunnerWire.clearTextBody("set-text", 0)
                return newFixedLengthResponse(Response.Status.OK, "application/json", body)
            }
        } else {
            focused?.recycle()
        }
        val deletes = FALLBACK_DELETE_COUNT
        runShellCommand(RunnerWire.deleteKeysCommand(deletes))
        device.waitForIdle(500)
        val body = RunnerWire.clearTextBody("key-events", deletes)
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveForeground(session: IHTTPSession): Response {
        // smix wire (HttpRunnerClient.foreground) sends {bundleId} per
        // iOS swift /foreground convention. Mirror that shape.
        val bundleId = RunnerWire.decodeForeground(readBodyString(session))
        // Android equivalent: `am start --activity-single-top -n
        // pkg/.MainActivity`. This brings the app to foreground without
        // launching a new instance (matches the iOS XCUIDevice activate
        // semantic).
        runShellCommand(RunnerWire.foregroundCommand(bundleId, entryPoint(bundleId)))
        device.waitForIdle(500)
        val body = RunnerWire.foregroundBody(bundleId)
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    // ---- /session/* ------------------------------------------------------
    //
    // Minimal session surface so the Kotlin SDK (driving through the FFI
    // session API → smix-runner-client) can talk to the Android runner.
    // Sessions on iOS cache an XCUIApplication binding to kill the
    // activation storm; Android has no per-request activation to
    // suppress, so a session here is pure bookkeeping: an id ↔ bundleId
    // binding that the lifecycle routes (launch/terminate/relaunch) act
    // on via the same shell-exec pathway /foreground uses.

    private val sessions = SessionTable()

    private fun serveSessionOpen(session: IHTTPSession): Response {
        val req = RunnerWire.decodeSessionOpen(readBodyString(session))
        if (req.bundleId.isEmpty()) {
            return sessionError(
                Response.Status.BAD_REQUEST,
                RunnerWire.sessionBadRequestBody("bundleId must be a non-empty string"),
            )
        }
        // iOS semantics: activate:true → foreground the target once,
        // synchronously, before returning; activatedOnce mirrors it.
        if (req.activate) {
            runShellCommand(RunnerWire.foregroundCommand(req.bundleId, entryPoint(req.bundleId)))
            device.waitForIdle(500)
        }
        val entry = sessions.open(req.bundleId, activated = req.activate)
        val body = RunnerWire.sessionOpenBody(entry.sessionId, req.activate, entry.openedAtMs)
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveSessionClose(session: IHTTPSession): Response {
        val sid = RunnerWire.decodeSessionId(readBodyString(session))
        if (sid.isEmpty()) {
            return sessionError(
                Response.Status.BAD_REQUEST,
                RunnerWire.sessionBadRequestBody("sessionId must be a non-empty string"),
            )
        }
        sessions.close(sid)
        return newFixedLengthResponse(
            Response.Status.OK,
            "application/json",
            RunnerWire.sessionCloseBody(),
        )
    }

    private fun serveSessionCloseAll(): Response {
        val closed = sessions.closeAll()
        return newFixedLengthResponse(
            Response.Status.OK,
            "application/json",
            RunnerWire.sessionCloseAllBody(closed),
        )
    }

    private fun serveSessionList(): Response {
        return newFixedLengthResponse(
            Response.Status.OK,
            "application/json",
            RunnerWire.sessionListBody(sessions.list()),
        )
    }

    private fun serveSessionRenewActivation(session: IHTTPSession): Response {
        val sid = RunnerWire.decodeSessionId(readBodyString(session))
        if (sid.isEmpty()) {
            return sessionError(
                Response.Status.BAD_REQUEST,
                RunnerWire.sessionBadRequestBody("sessionId must be a non-empty string"),
            )
        }
        val entry = sessions.renewActivation(sid) ?: return sessionError(
            Response.Status.NOT_FOUND,
            RunnerWire.sessionNotFoundBody("unknown session id"),
        )
        runShellCommand(RunnerWire.foregroundCommand(entry.bundleId, entryPoint(entry.bundleId)))
        device.waitForIdle(500)
        return newFixedLengthResponse(
            Response.Status.OK,
            "application/json",
            RunnerWire.sessionRenewBody(activated = true),
        )
    }

    private fun serveSessionAppLifecycle(session: IHTTPSession, terminate: Boolean): Response {
        // Body carries sessionId (+ args/env/waitFor*Ms, accepted and
        // ignored — XCUITest launch-injection has no `am` equivalent).
        val sid = RunnerWire.decodeSessionId(readBodyString(session))
        if (sid.isEmpty()) {
            return sessionError(
                Response.Status.BAD_REQUEST,
                RunnerWire.sessionBadRequestBody("sessionId must be a non-empty string"),
            )
        }
        val entry = sessions.get(sid) ?: return sessionError(
            Response.Status.NOT_FOUND,
            RunnerWire.sessionNotFoundBody("unknown session id"),
        )
        val start = System.currentTimeMillis()
        val cmd = if (terminate) {
            RunnerWire.terminateAppCommand(entry.bundleId)
        } else {
            RunnerWire.foregroundCommand(entry.bundleId, entryPoint(entry.bundleId))
        }
        runShellCommand(cmd)
        device.waitForIdle(500)
        val wallMs = System.currentTimeMillis() - start
        return newFixedLengthResponse(
            Response.Status.OK,
            "application/json",
            RunnerWire.sessionLifecycleBody(true, wallMs),
        )
    }

    private fun serveSessionRelaunchApp(session: IHTTPSession): Response {
        val sid = RunnerWire.decodeSessionId(readBodyString(session))
        if (sid.isEmpty()) {
            return sessionError(
                Response.Status.BAD_REQUEST,
                RunnerWire.sessionBadRequestBody("sessionId must be a non-empty string"),
            )
        }
        val entry = sessions.get(sid) ?: return sessionError(
            Response.Status.NOT_FOUND,
            RunnerWire.sessionNotFoundBody("unknown session id"),
        )
        val start = System.currentTimeMillis()
        runShellCommand(RunnerWire.terminateAppCommand(entry.bundleId))
        runShellCommand(RunnerWire.foregroundCommand(entry.bundleId, entryPoint(entry.bundleId)))
        device.waitForIdle(500)
        val wallMs = System.currentTimeMillis() - start
        return newFixedLengthResponse(
            Response.Status.OK,
            "application/json",
            RunnerWire.sessionRelaunchBody(true, wallMs),
        )
    }

    private fun sessionError(status: Response.Status, body: String): Response =
        newFixedLengthResponse(status, "application/json", body)

    /// The activity `am start -n` should name for this package.
    ///
    /// Every launch route used to hard-code `.MainActivity`, which is
    /// what a scaffolded app is called and what almost nothing else is:
    /// an AOSP app, or one whose entry point was renamed, simply did
    /// not start, and the response said nothing about why. The package
    /// manager already knows, and the runner has a Context to ask with.
    ///
    /// Returns null when it cannot answer, which
    /// `RunnerWire.foregroundCommand` reads as "use the old
    /// convention" — no device that worked before is worse off.
    private fun entryPoint(bundleId: String): String? {
        val pm = instrumentation.targetContext.packageManager
        val component = pm.getLaunchIntentForPackage(bundleId)?.component ?: return null
        val name = component.className
        return if (name.startsWith("$bundleId.")) {
            name.removePrefix(bundleId)
        } else {
            name
        }
    }

    private fun runShellCommand(cmd: String) {
        val pfd = instrumentation.uiAutomation.executeShellCommand(cmd)
        try {
            // Drain so the shell command completes before we return.
            val buf = ByteArray(256)
            val input = java.io.FileInputStream(pfd.fileDescriptor)
            while (input.read(buf) >= 0) { /* drain */ }
        } finally {
            pfd.close()
        }
    }

    private fun serveFindTextByOcr(session: IHTTPSession): Response {
        // locales + recognition_level are iOS-specific Apple Vision args;
        // the ML Kit Latin script package handles ASCII/Latin universally
        // so they are accepted but ignored here. Dedicated CJK packages
        // can be added when the app under test needs them
        // (text-recognition-chinese / japanese / korean / devanagari
        // packages add ~10MB each).
        val target = RunnerWire.decodeOcrTarget(readBodyString(session))

        val bitmap = pacedScreenshot()
            ?: return errorJson(
                Response.Status.INTERNAL_ERROR,
                "screenshot_failed",
                "UiAutomation.takeScreenshot returned null",
            )
        val width = bitmap.width
        val height = bitmap.height

        val recognizer = TextRecognition.getClient(TextRecognizerOptions.DEFAULT_OPTIONS)
        val image = InputImage.fromBitmap(bitmap, /* rotationDegrees = */ 0)

        // ML Kit is callback-based; block this thread until result lands.
        val latch = JCountDownLatch(1)
        var matchedRect: Rect? = null
        var error: Throwable? = null
        recognizer.process(image)
            .addOnSuccessListener { result ->
                try {
                    val lowerTarget = target.lowercase()
                    for (block in result.textBlocks) {
                        for (line in block.lines) {
                            if (line.text.lowercase().contains(lowerTarget)) {
                                matchedRect = line.boundingBox
                                return@addOnSuccessListener
                            }
                        }
                    }
                } finally {
                    latch.countDown()
                }
            }
            .addOnFailureListener { e ->
                error = e
                latch.countDown()
            }
            .addOnCompleteListener {
                // safety: ensure latch released even on exception in success
                if (latch.count > 0) latch.countDown()
            }
        latch.await(5, TimeUnit.SECONDS)
        bitmap.recycle()

        error?.let {
            return errorJson(
                Response.Status.INTERNAL_ERROR,
                "ocr_failed",
                "ML Kit text recognition: ${it.message}",
            )
        }

        val rect = matchedRect
        val body = if (rect != null && width > 0 && height > 0) {
            RunnerWire.ocrFoundBody(rect.left, rect.top, rect.right, rect.bottom, width, height)
        } else {
            RunnerWire.ocrNotFoundBody()
        }
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveSystemPopups(): Response {
        // Walk UiAutomation.windows and classify each as
        // app | dialog | system based on getType() / window class.
        // Compose AlertDialog typically reports type = TYPE_APPLICATION
        // but is hosted in a separate window with isDialog=true (API 24+).
        // We surface every TYPE_APPLICATION_PANEL / non-main window as a
        // candidate popup; the host driver then matches on title/body to
        // disambiguate which one it cares about.
        val automation = instrumentation.uiAutomation
        val popupsArr = JSONArray()
        var idx = 0
        for (window in automation.windows) {
            val root = window.root ?: continue
            try {
                if (!PopupClassifier.isDialogLike(window, root)) continue
                val title = PopupClassifier.findTitle(root)
                val body = PopupClassifier.findBody(root, title)
                val buttons = PopupClassifier.collectButtons(root)
                val popupId = "android-popup-$idx"
                idx += 1
                popupsArr.put(
                    PopupWire.popupEntry(
                        popupId,
                        root.packageName?.toString() ?: "",
                        title,
                        body,
                        buttons,
                    ),
                )
            } finally {
                root.recycle()
            }
        }
        val body = RunnerWire.popupsBody(popupsArr)
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveSystemPopupAction(session: IHTTPSession): Response {
        val (popupId, buttonId) = RunnerWire.decodeSystemPopupAction(readBodyString(session))
        // Re-walk to find the popup + button (mirror swift smix-runner
        // re-resolve semantics — stale ids surface as ok:false).
        val automation = instrumentation.uiAutomation
        var idx = 0
        var clicked = false
        for (window in automation.windows) {
            val root = window.root ?: continue
            try {
                if (!PopupClassifier.isDialogLike(window, root)) continue
                val currentId = "android-popup-$idx"
                idx += 1
                if (currentId != popupId) continue
                // Find button matching button_id (we use Compose testTag /
                // viewIdResourceName as button_id).
                val target = PopupClassifier.findButton(root, buttonId) ?: continue
                try {
                    val rect = Rect()
                    target.getBoundsInScreen(rect)
                    if (rect.width() > 0 && rect.height() > 0) {
                        device.click(rect.centerX(), rect.centerY())
                        device.waitForIdle(500)
                        clicked = true
                    }
                } finally {
                    target.recycle()
                }
                break
            } finally {
                root.recycle()
            }
        }
        val body = RunnerWire.popupActionBody(clicked, popupId, buttonId)
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveWebViewEvalProxy(session: IHTTPSession): Response {
        // Runner and app under test live in separate processes; the
        // runner can't access the app's WebView instance directly. Proxy
        // HTTP to the app's shim server on port 28081 (started by
        // MainActivity onCreate via WebViewEvalServer).
        val files = HashMap<String, String>()
        session.parseBody(files)
        val payload = files["postData"] ?: "{}"
        return try {
            val url = java.net.URL("http://127.0.0.1:28081/webview-eval")
            val conn = url.openConnection() as java.net.HttpURLConnection
            conn.requestMethod = "POST"
            conn.setRequestProperty("Content-Type", "application/json")
            conn.doOutput = true
            conn.connectTimeout = 5_000
            conn.readTimeout = 6_000
            conn.outputStream.use { it.write(payload.toByteArray(Charsets.UTF_8)) }
            val status = conn.responseCode
            val stream = if (status in 200..299) conn.inputStream else conn.errorStream
            val body = stream.bufferedReader(Charsets.UTF_8).use { it.readText() }
            val statusEnum = when (status) {
                200 -> Response.Status.OK
                400 -> Response.Status.BAD_REQUEST
                500 -> Response.Status.INTERNAL_ERROR
                else -> Response.Status.lookup(status) ?: Response.Status.INTERNAL_ERROR
            }
            newFixedLengthResponse(statusEnum, "application/json", body)
        } catch (e: Throwable) {
            val body = RunnerWire.proxyFailedBody(e.message ?: e.javaClass.simpleName)
            newFixedLengthResponse(Response.Status.INTERNAL_ERROR, "application/json", body)
        }
    }

    // The body was already consumed at the top of `serve` — NanoHTTPD
    // can only parse it once, and it MUST be consumed there so the next
    // request on this keep-alive connection is not prefixed with it.
    private fun readBodyString(session: IHTTPSession): String {
        val body = drainedBody.get()
        if (!body.isNullOrEmpty()) return body
        return session.queryParameterString ?: "{}"
    }

    private fun errorJson(status: Response.Status, kind: String, message: String): Response {
        val body = RunnerWire.errorBody(kind, message)
        return newFixedLengthResponse(status, "application/json", body)
    }

    private fun serveNotImplemented(uri: String, method: String): Response {
        val body = RunnerWire.notImplementedBody(uri, method)
        return newFixedLengthResponse(
            Response.Status.NOT_IMPLEMENTED,
            "application/json",
            body,
        )
    }
}

/// Classify a window as dialog-like + walk title/body/buttons. Compose
/// AlertDialog hosts in a separate window with `isDialog=true` OR
/// `type=APPLICATION_PANEL` on API 24+. Fallback heuristic: small window
/// bounds (not full-screen) + has button-like children.
object PopupClassifier {
    fun isDialogLike(window: AccessibilityWindowInfo, root: AccessibilityNodeInfo): Boolean {
        // Primary: API isDialog flag (24+ stable signal).
        if (window.type == AccessibilityWindowInfo.TYPE_APPLICATION) {
            // For TYPE_APPLICATION, exclude main app window. We classify as
            // dialog if a Button child is found AND the window is NOT the
            // largest one (heuristic — main app window typically full-screen).
            val rect = Rect()
            root.getBoundsInScreen(rect)
            val screenW = window.root?.let { Rect().also { r -> it.getBoundsInScreen(r); }.right - rect.left } ?: 0
            // Dialog if it has any Button-class descendant AND the window
            // doesn't span full screen height (>90% means main app).
            if (rect.height() > 0 && rect.height() < screenW * 2) {
                // Allow — check for button presence
                return hasButtonDescendant(root)
            }
        }
        return false
    }

    private fun hasButtonDescendant(node: AccessibilityNodeInfo): Boolean {
        val cls = node.className?.toString() ?: ""
        if (cls.endsWith("Button") || cls.contains("Button")) return true
        // testTag-based heuristic for Compose
        node.viewIdResourceName?.let { id ->
            if (id.endsWith("-btn") || id.contains("button")) return true
        }
        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            try {
                if (hasButtonDescendant(child)) return true
            } finally {
                child.recycle()
            }
        }
        return false
    }

    /// Find title — first TextView/Text child within dialog that's NOT a
    /// button label.
    fun findTitle(root: AccessibilityNodeInfo): String? {
        return findFirstNonButtonText(root)
    }

    /// Find body — second TextView (after title), skipping button labels.
    fun findBody(root: AccessibilityNodeInfo, title: String?): String? {
        return findFirstNonButtonText(root, skipText = title)
    }

    private fun findFirstNonButtonText(
        node: AccessibilityNodeInfo,
        skipText: String? = null,
        seen: MutableSet<String> = mutableSetOf(),
    ): String? {
        val cls = node.className?.toString() ?: ""
        val text = node.text?.toString()
        if (text != null && text.isNotEmpty() && !cls.endsWith("Button")) {
            if (text != skipText && !seen.contains(text)) {
                seen.add(text)
                return text
            }
        }
        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            try {
                val found = findFirstNonButtonText(child, skipText, seen)
                if (found != null) return found
            } finally {
                child.recycle()
            }
        }
        return null
    }

    /// Walk all button-class descendants + return JSONArray of {id, label,
    /// role, destructive}.
    fun collectButtons(root: AccessibilityNodeInfo): JSONArray {
        val arr = JSONArray()
        collectButtonsRecursive(root, arr)
        return arr
    }

    private fun collectButtonsRecursive(node: AccessibilityNodeInfo, arr: JSONArray) {
        val cls = node.className?.toString() ?: ""
        val viewId = node.viewIdResourceName ?: ""
        val isButton = cls.endsWith("Button") || cls.contains("Button") ||
            viewId.endsWith("-btn") || viewId.contains("button")
        if (isButton) {
            val short = viewId.substringAfter(":id/", viewId)
            // Compose AlertDialog wraps the Button class node in a parent
            // ViewGroup that ALSO contains a sibling TextView holding the
            // visible label. node.text on the Button itself is empty; walk
            // siblings via getParent() for label.
            val labelFromSelf = node.text?.toString().orEmpty()
            val labelFromDesc = node.contentDescription?.toString().orEmpty()
            val labelFromSibling = if (labelFromSelf.isEmpty() && labelFromDesc.isEmpty()) {
                findSiblingText(node)
            } else {
                null
            }
            val label = labelFromSelf.takeIf { it.isNotEmpty() }
                ?: labelFromDesc.takeIf { it.isNotEmpty() }
                ?: labelFromSibling
                ?: ""
            val id = PopupWire.buttonId(short, label)
            val role = PopupWire.buttonRole(id, label)
            arr.put(PopupWire.buttonEntry(id, label, role))
        }
        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            try {
                collectButtonsRecursive(child, arr)
            } finally {
                child.recycle()
            }
        }
    }

    /// Compose pattern: TextButton renders as a ViewGroup containing a
    /// TextView (label) + a Button-class node (interactable). Get parent
    /// + look at siblings for a TextView with text.
    private fun findSiblingText(buttonNode: AccessibilityNodeInfo): String? {
        val parent = buttonNode.parent ?: return null
        try {
            for (i in 0 until parent.childCount) {
                val sibling = parent.getChild(i) ?: continue
                try {
                    val sCls = sibling.className?.toString() ?: ""
                    val sText = sibling.text?.toString()
                    if (sText != null && sText.isNotEmpty() && !sCls.endsWith("Button")) {
                        return sText
                    }
                } finally {
                    sibling.recycle()
                }
            }
        } finally {
            parent.recycle()
        }
        return null
    }

    /// Find button by id (testTag-derived OR label-lowercase fallback).
    /// Caller must recycle().
    fun findButton(root: AccessibilityNodeInfo, buttonId: String): AccessibilityNodeInfo? {
        val cls = root.className?.toString() ?: ""
        val viewId = root.viewIdResourceName ?: ""
        val short = viewId.substringAfter(":id/", viewId)
        val isButton = cls.endsWith("Button") || cls.contains("Button")
        if (isButton) {
            // Match by testTag-derived id first, then by label-slug fallback
            // (Compose AlertDialog doesn't propagate testTagsAsResourceId
            // into its dialog window — host driver gets label-slug as id).
            val matchById = short.isNotEmpty() && short == buttonId
            val labelFromSibling = findSiblingText(root) ?: ""
            val matchByLabel = labelFromSibling.isNotEmpty() &&
                labelFromSibling.lowercase() == buttonId.lowercase()
            if (matchById || matchByLabel) {
                return AccessibilityNodeInfo.obtain(root)
            }
        }
        for (i in 0 until root.childCount) {
            val child = root.getChild(i) ?: continue
            val found: AccessibilityNodeInfo? = try {
                findButton(child, buttonId)
            } finally {
                child.recycle()
            }
            if (found != null) return found
        }
        return null
    }
}

/// Build A11yNode JSON tree from UiAutomation windows. Walks every
/// attached window's AccessibilityNodeInfo tree and merges under a
/// virtual root so the host driver sees foreground app + system windows
/// in a single dump.
object TreeBuilder {
    fun fromWindows(automation: UiAutomation): JSONObject {
        val rootChildren = JSONArray()
        var maxW = 0
        var maxH = 0
        var unreadable = 0
        for (window in automation.windows) {
            // A window whose root cannot be read is counted, not
            // dropped in silence: absent from the tree and "that app
            // has no accessibility nodes" look identical to the caller,
            // and one of them is smix's problem.
            val node = window.root
            if (node == null) {
                unreadable += 1
                continue
            }
            try {
                val obj = nodeToJson(node)
                // A window's own type decides one role that no node class
                // can: the input-method window is the software keyboard.
                // Without this the tree had no keyboard in it at all,
                // while `keyboardIsUp()` two hundred lines up has been
                // reading the very same window type for releases — the
                // capability was in the driver and not in the tree, which
                // is the one place every verb can reach it from.
                TreeWire.roleForWindowType(window.type)?.let { obj.put("role", it) }
                rootChildren.put(obj)
                val bounds = Rect()
                node.getBoundsInScreen(bounds)
                if (bounds.right > maxW) maxW = bounds.right
                if (bounds.bottom > maxH) maxH = bounds.bottom
            } finally {
                node.recycle()
            }
        }
        return TreeWire.windowRootJson(maxW, maxH, rootChildren, unreadable)
    }

    private fun nodeToJson(node: AccessibilityNodeInfo): JSONObject {
        // Per-node refresh parity with the /tap-by-id walkFind path.
        // Chained NavHost transitions leave cached AccessibilityNodeInfo
        // descendants stale even after parent root.refresh(); a tree
        // dump without per-node refresh reports the pre-transition
        // state while screencap shows the next screen actually
        // rendered. Refreshing per-node forces the framework to re-sync
        // each child with current Compose semantics.
        node.refresh()
        val rect = Rect()
        node.getBoundsInScreen(rect)
        val childArr = JSONArray()
        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            try {
                childArr.put(nodeToJson(child))
            } finally {
                child.recycle()
            }
        }
        return TreeWire.nodeJson(
            rawType = node.className?.toString() ?: "",
            identifier = node.viewIdResourceName,
            label = node.contentDescription?.toString(),
            text = node.text?.toString(),
            x = rect.left,
            y = rect.top,
            w = rect.right - rect.left,
            h = rect.bottom - rect.top,
            enabled = node.isEnabled,
            selected = node.isSelected,
            hasFocus = node.isFocused,
            visible = node.isVisibleToUser || (rect.width() > 0 && rect.height() > 0),
            children = childArr,
        )
    }
}

/// Parse UiAutomator2 dumpWindowHierarchy XML → smix A11yNode JSON.
///
/// Mapping (Android XML attribute → A11yNode JSON field):
/// - class                → rawType
/// - (derived from class) → role (Button/TextField/Image/Switch/CheckBox/StaticText etc.)
/// - resource-id          → identifier (last segment after ":id/" if prefixed)
/// - content-desc         → label
/// - text                 → text
/// - bounds="[x1,y1][x2,y2]" → bounds: {x: x1, y: y1, w: x2-x1, h: y2-y1}
/// - enabled              → enabled
/// - selected             → selected
/// - focused              → hasFocus
/// - visible-to-user      → visible (else derived from bounds non-empty)
/// - children             → children (recursive)
object TreeXmlParser {
    private val BOUNDS_RX = Regex("""\[(-?\d+),(-?\d+)\]\[(-?\d+),(-?\d+)\]""")

    fun parse(xml: ByteArray): JSONObject {
        val factory = XmlPullParserFactory.newInstance()
        factory.isNamespaceAware = false
        val parser = factory.newPullParser()
        parser.setInput(ByteArrayInputStream(xml), "UTF-8")

        var event = parser.eventType
        val stack = ArrayDeque<JSONObject>()
        var root: JSONObject? = null

        while (event != XmlPullParser.END_DOCUMENT) {
            when (event) {
                XmlPullParser.START_TAG -> {
                    if (parser.name == "hierarchy") {
                        // Root <hierarchy rotation="N"> wraps the top-level node.
                        // Skip; the next <node ...> is the real root.
                    } else if (parser.name == "node") {
                        val obj = JSONObject()
                        val cls = parser.getAttributeValue(null, "class") ?: ""
                        obj.put("rawType", cls)
                        TreeWire.deriveRole(cls)?.let { obj.put("role", it) }

                        parser.getAttributeValue(null, "resource-id")
                            ?.takeIf { it.isNotEmpty() }
                            ?.let { obj.put("identifier", TreeWire.shortResourceId(it)) }
                        parser.getAttributeValue(null, "content-desc")
                            ?.takeIf { it.isNotEmpty() }
                            ?.let { obj.put("label", it) }
                        parser.getAttributeValue(null, "text")
                            ?.takeIf { it.isNotEmpty() }
                            ?.let { obj.put("text", it) }

                        val boundsStr = parser.getAttributeValue(null, "bounds") ?: ""
                        obj.put("bounds", parseBounds(boundsStr))

                        obj.put("enabled", parser.getAttributeValue(null, "enabled") == "true")
                        obj.put("selected", parser.getAttributeValue(null, "selected") == "true")
                        obj.put("hasFocus", parser.getAttributeValue(null, "focused") == "true")

                        // visible: prefer visible-to-user when present, else
                        // derive from bounds (non-zero w/h).
                        val visibleAttr = parser.getAttributeValue(null, "visible-to-user")
                        val visible = when {
                            visibleAttr == "true" -> true
                            visibleAttr == "false" -> false
                            else -> {
                                val b = obj.getJSONObject("bounds")
                                b.optDouble("w", 0.0) > 0 && b.optDouble("h", 0.0) > 0
                            }
                        }
                        obj.put("visible", visible)

                        obj.put("children", JSONArray())

                        if (stack.isEmpty()) {
                            root = obj
                        } else {
                            stack.last().getJSONArray("children").put(obj)
                        }
                        stack.addLast(obj)
                    }
                }
                XmlPullParser.END_TAG -> {
                    if (parser.name == "node") {
                        stack.removeLast()
                    }
                }
            }
            event = parser.next()
        }
        // If no <node> seen (empty hierarchy / weird state), emit a minimal
        // placeholder root so /tree always returns a valid A11yNode shape.
        return root ?: JSONObject()
            .put("rawType", "android.view.View")
            .put("bounds", JSONObject().put("x", 0).put("y", 0).put("w", 0).put("h", 0))
            .put("enabled", false)
            .put("selected", false)
            .put("hasFocus", false)
            .put("visible", false)
            .put("children", JSONArray())
    }

    private fun parseBounds(s: String): JSONObject {
        val m = BOUNDS_RX.find(s)
        if (m != null) {
            val (x1, y1, x2, y2) = m.destructured
            val ix1 = x1.toInt(); val iy1 = y1.toInt()
            val ix2 = x2.toInt(); val iy2 = y2.toInt()
            return JSONObject()
                .put("x", ix1)
                .put("y", iy1)
                .put("w", (ix2 - ix1).coerceAtLeast(0))
                .put("h", (iy2 - iy1).coerceAtLeast(0))
        }
        return JSONObject().put("x", 0).put("y", 0).put("w", 0).put("h", 0)
    }
}
