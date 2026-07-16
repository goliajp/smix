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
import android.view.KeyEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityWindowInfo
import com.google.mlkit.vision.common.InputImage
import com.google.mlkit.vision.text.TextRecognition
import com.google.mlkit.vision.text.latin.TextRecognizerOptions
import java.util.concurrent.CountDownLatch as JCountDownLatch
import java.util.concurrent.TimeUnit
import androidx.test.ext.junit.runners.AndroidJUnit4
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
        inst.uiAutomation.serviceInfo = info
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
    // /tree serve counter for the X-Tree-Snapshot-Refresh-Count
    // header (parity with the iOS runner).
    private val treeServeCounter = java.util.concurrent.atomic.AtomicLong(0)

    override fun serve(session: IHTTPSession): Response {
        val uri = session.uri
        return try {
            when {
                uri == "/health" && session.method == Method.GET -> serveHealth()
                uri == "/tree" && session.method == Method.GET -> serveTree()
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
                uri == "/foreground" && session.method == Method.POST -> serveForeground(session)
                uri == "/find-text-by-ocr" && session.method == Method.POST ->
                    serveFindTextByOcr(session)
                uri == "/system-popups" && session.method == Method.GET -> serveSystemPopups()
                uri == "/system-popup-action" && session.method == Method.POST ->
                    serveSystemPopupAction(session)
                uri == "/webview-eval" && session.method == Method.POST ->
                    serveWebViewEvalProxy(session)
                else -> serveNotImplemented(uri, session.method.name)
            }
        } catch (e: Throwable) {
            val body = JSONObject()
                .put("error", "internal_error")
                .put("message", e.message ?: e.javaClass.simpleName)
                .put("class", e.javaClass.name)
                .toString()
            newFixedLengthResponse(Response.Status.INTERNAL_ERROR, "application/json", body)
        }
    }

    private fun serveHealth(): Response {
        val body = JSONObject()
            .put("status", "ok")
            .put("runner", "smix-android-runner")
            .put("version", SmixRunner.VERSION)
            .toString()
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

    private fun serveTapAtNormCoord(session: IHTTPSession): Response {
        val files = HashMap<String, String>()
        session.parseBody(files)
        val payload = files["postData"] ?: session.queryParameterString ?: "{}"
        val req = JSONObject(payload)
        val nx = req.getDouble("nx")
        val ny = req.getDouble("ny")
        val w = device.displayWidth
        val h = device.displayHeight
        val px = (nx * w).toInt().coerceIn(0, w - 1)
        val py = (ny * h).toInt().coerceIn(0, h - 1)
        val ok = device.click(px, py)
        // Give the dispatched IO event time to render before /tree probes
        // observe the post-tap UI state.
        device.waitForIdle(500)
        val body = JSONObject()
            .put("status", if (ok) "ok" else "click_returned_false")
            .put("displayWidth", w)
            .put("displayHeight", h)
            .put("x", px)
            .put("y", py)
            .toString()
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveSwipeAtNormCoord(session: IHTTPSession): Response {
        val req = readJsonBody(session)
        val fromNx = req.getDouble("fromNx")
        val fromNy = req.getDouble("fromNy")
        val toNx = req.getDouble("toNx")
        val toNy = req.getDouble("toNy")
        val w = device.displayWidth
        val h = device.displayHeight
        val x1 = (fromNx * w).toInt().coerceIn(0, w - 1)
        val y1 = (fromNy * h).toInt().coerceIn(0, h - 1)
        val x2 = (toNx * w).toInt().coerceIn(0, w - 1)
        val y2 = (toNy * h).toInt().coerceIn(0, h - 1)
        // 50 steps ≈ 500ms — enough to register as fling/scroll, not so
        // fast it's interpreted as a tap.
        val ok = device.swipe(x1, y1, x2, y2, 50)
        device.waitForIdle(500)
        val body = JSONObject()
            .put("status", if (ok) "ok" else "swipe_returned_false")
            .put("from", JSONObject().put("x", x1).put("y", y1))
            .put("to", JSONObject().put("x", x2).put("y", y2))
            .toString()
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveSwipeOnce(session: IHTTPSession): Response {
        val req = readJsonBody(session)
        val direction = req.getString("direction")
        val w = device.displayWidth
        val h = device.displayHeight
        // Use 30%-70% of the screen along the swipe axis. Cross-axis fixed
        // at midline.
        //
        // Maestro navigation convention (`SwipeDirection` enum docstring):
        // the wire "direction" names what content to SEE, not the finger
        // gesture direction. `down` = "navigate down" = "see below" =
        // content moves up = finger gestures up (start at y=70%, end at
        // y=30%).
        val (x1, y1, x2, y2) = when (direction) {
            // see above ← finger gestures down
            "up" -> intArrayOf(w / 2, (h * 0.3).toInt(), w / 2, (h * 0.7).toInt())
            // see below ← finger gestures up
            "down" -> intArrayOf(w / 2, (h * 0.7).toInt(), w / 2, (h * 0.3).toInt())
            // see left  ← finger gestures right
            "left" -> intArrayOf((w * 0.3).toInt(), h / 2, (w * 0.7).toInt(), h / 2)
            // see right ← finger gestures left
            "right" -> intArrayOf((w * 0.7).toInt(), h / 2, (w * 0.3).toInt(), h / 2)
            else -> return errorJson(
                Response.Status.BAD_REQUEST,
                "bad_direction",
                "expected one of up/down/left/right, got '$direction'",
            )
        }.let { arr -> IntQuad(arr[0], arr[1], arr[2], arr[3]) }
        val ok = device.swipe(x1, y1, x2, y2, 50)
        device.waitForIdle(500)
        val body = JSONObject()
            .put("status", if (ok) "ok" else "swipe_returned_false")
            .put("direction", direction)
            .put("from", JSONObject().put("x", x1).put("y", y1))
            .put("to", JSONObject().put("x", x2).put("y", y2))
            .toString()
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun servePressKey(session: IHTTPSession): Response {
        val req = readJsonBody(session)
        val key = req.getString("key")
        val code = KeyMap.androidKeyCode(key) ?: return errorJson(
            Response.Status.BAD_REQUEST,
            "unknown_key",
            "no Android KeyEvent mapping for '$key'",
        )
        device.pressKeyCode(code)
        device.waitForIdle(500)
        val body = JSONObject()
            .put("status", "ok")
            .put("key", key)
            .put("keyCode", code)
            .toString()
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveBack(): Response {
        val ok = device.pressBack()
        device.waitForIdle(500)
        val body = JSONObject()
            .put("status", if (ok) "ok" else "press_back_returned_false")
            .toString()
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveHideKeyboard(): Response {
        // No first-class UiAutomator2 hide-keyboard; closest is pressBack
        // (closes IME without dismissing app). Mirror iOS swift handler
        // semantics (no harm if no keyboard is up).
        device.pressBack()
        device.waitForIdle(500)
        val body = JSONObject().put("status", "ok").toString()
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveSetOrientation(session: IHTTPSession): Response {
        val req = readJsonBody(session)
        val orientation = req.getString("orientation")
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
        val body = JSONObject()
            .put("status", "ok")
            .put("orientation", orientation)
            .toString()
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveTapById(session: IHTTPSession): Response {
        val req = readJsonBody(session)
        val id = req.getString("id")
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
        while (System.currentTimeMillis() < pollDeadlineMs && !clicked) {
            val targetNode = findNodeByViewId(id)
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
        val body = JSONObject()
            .put("ok", clicked)
            .put("id", id)
            .put("path", path)
            .put("saw_node", sawNode)
            .put("saw_action_click", sawActionClick)
            .toString()
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    /// Find first node matching `viewIdResourceName`. Tries common
    /// package prefixes (`com.example.app` / `dev.smix.runner.test`)
    /// before short-id strict. Caller owns the returned node and MUST
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
    private fun findNodeByViewId(shortId: String): AccessibilityNodeInfo? {
        // Compose `testTagsAsResourceId = true` emits the bare short
        // string (no `<pkg>:id/` prefix) on some layouts (e.g. FlowRow)
        // and `<pkg>:id/<short>` on others (older LazyRow). Try bare
        // first (common case), then package-qualified (SUT + runner
        // test process) for each window.
        val candidates = buildList {
            add(shortId)
            add("com.example.app:id/$shortId")
            add("dev.smix.runner.test:id/$shortId")
        }
        for (window in instrumentation.uiAutomation.windows) {
            val root = window.root ?: continue
            for (cand in candidates) {
                val hits = root.findAccessibilityNodeInfosByViewId(cand)
                if (hits.isNotEmpty()) {
                    val node = hits[0]
                    hits.drop(1).forEach { it.recycle() }
                    return node
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
            if (found != null) return found
        }
        return null
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
        val req = readJsonBody(session)
        val nx = req.getDouble("nx")
        val ny = req.getDouble("ny")
        val w = device.displayWidth
        val h = device.displayHeight
        val px = (nx * w).toInt().coerceIn(0, w - 1)
        val py = (ny * h).toInt().coerceIn(0, h - 1)
        device.click(px, py)
        // Standard double-tap inter-tap window — 150ms is below most
        // systems' DOUBLE_TAP_TIMEOUT (300ms) so events register as a
        // pair, not as two separate taps.
        Thread.sleep(150)
        device.click(px, py)
        device.waitForIdle(500)
        val body = JSONObject()
            .put("status", "ok")
            .put("x", px)
            .put("y", py)
            .toString()
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveLongPressAtNormCoord(session: IHTTPSession): Response {
        val req = readJsonBody(session)
        val nx = req.getDouble("nx")
        val ny = req.getDouble("ny")
        val durationMs = req.optLong("durationMs", 500L)
        val w = device.displayWidth
        val h = device.displayHeight
        val px = (nx * w).toInt().coerceIn(0, w - 1)
        val py = (ny * h).toInt().coerceIn(0, h - 1)
        // UiDevice.swipe(x, y, x, y, steps) — same start/end coord with
        // explicit step count approximates a long press; each step adds
        // ~5ms, so steps = duration / 5 ms.
        val steps = (durationMs / 5).toInt().coerceAtLeast(1)
        device.swipe(px, py, px, py, steps)
        device.waitForIdle(500)
        val body = JSONObject()
            .put("status", "ok")
            .put("x", px)
            .put("y", py)
            .put("durationMs", durationMs)
            .toString()
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveInputText(session: IHTTPSession): Response {
        val req = readJsonBody(session)
        val text = req.getString("text")
        // `adb shell input text` (mirrored via Instrumentation
        // executeShellCommand) types into whichever field is currently
        // focused. Caller must have tapped to focus first (AndroidDriver
        // orchestrates host-resolve → tap → input-text).
        //
        // Android `input text` interprets unescaped spaces as separators.
        // UiAutomation.executeShellCommand does NOT run through `sh -c`;
        // it splits on whitespace + execs directly, so quote characters
        // are typed literally. Use bare `input text` + escape spaces to
        // `%s` (input cmd convention).
        val escaped = text
            .replace("\\", "\\\\")
            .replace(" ", "%s")
        val cmd = "input text $escaped"
        val pfd = instrumentation.uiAutomation.executeShellCommand(cmd)
        try {
            // Drain so the shell command completes before we return.
            val buf = ByteArray(256)
            val input = java.io.FileInputStream(pfd.fileDescriptor)
            while (input.read(buf) >= 0) { /* drain */ }
        } finally {
            pfd.close()
        }
        device.waitForIdle(500)
        val body = JSONObject()
            .put("status", "ok")
            .put("text", text)
            .toString()
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveForeground(session: IHTTPSession): Response {
        val req = readJsonBody(session)
        // smix wire (HttpRunnerClient.foreground) sends {bundleId} per
        // iOS swift /foreground convention. Mirror that shape.
        val bundleId = req.getString("bundleId")
        // Android equivalent: `am start --activity-single-top -n
        // pkg/.MainActivity`. This brings the app to foreground without
        // launching a new instance (matches the iOS XCUIDevice activate
        // semantic).
        val cmd = "am start --activity-single-top -n $bundleId/.MainActivity"
        val pfd = instrumentation.uiAutomation.executeShellCommand(cmd)
        try {
            val buf = ByteArray(256)
            val input = java.io.FileInputStream(pfd.fileDescriptor)
            while (input.read(buf) >= 0) { /* drain */ }
        } finally {
            pfd.close()
        }
        device.waitForIdle(500)
        val body = JSONObject()
            .put("status", "ok")
            .put("bundleId", bundleId)
            .toString()
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveFindTextByOcr(session: IHTTPSession): Response {
        val req = readJsonBody(session)
        val target = req.getString("text")
        // locales + recognition_level are iOS-specific Apple Vision args;
        // the ML Kit Latin script package handles ASCII/Latin universally
        // so we read but ignore them here. Dedicated CJK packages can be
        // added when the app under test needs them
        // (text-recognition-chinese / japanese / korean / devanagari
        // packages add ~10MB each).
        @Suppress("UNUSED_VARIABLE")
        val locales = req.optJSONArray("locales")
        @Suppress("UNUSED_VARIABLE")
        val level = req.optString("recognition_level", "accurate")

        val bitmap = instrumentation.uiAutomation.takeScreenshot()
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
            val nx = rect.left.toDouble() / width
            val ny = rect.top.toDouble() / height
            val w = (rect.right - rect.left).toDouble() / width
            val h = (rect.bottom - rect.top).toDouble() / height
            // Wire shape matches iOS swift /find-text-by-ocr response:
            // {found: bool, frame: [nx, ny, w, h]} per HttpRunnerClient
            // deserialization.
            JSONObject()
                .put("found", true)
                .put("frame", JSONArray().put(nx).put(ny).put(w).put(h))
                .toString()
        } else {
            JSONObject().put("found", false).toString()
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
                    JSONObject()
                        .put("id", popupId)
                        .put("type", "alert")
                        .put("source", root.packageName?.toString() ?: "")
                        .put("title", title ?: "")
                        .put("body", body ?: "")
                        .put("buttons", buttons),
                )
            } finally {
                root.recycle()
            }
        }
        val body = JSONObject().put("popups", popupsArr).toString()
        return newFixedLengthResponse(Response.Status.OK, "application/json", body)
    }

    private fun serveSystemPopupAction(session: IHTTPSession): Response {
        val req = readJsonBody(session)
        val popupId = req.getString("popup_id")
        val buttonId = req.getString("button_id")
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
        val body = JSONObject()
            .put("ok", clicked)
            .put("popupId", popupId)
            .put("buttonId", buttonId)
            .toString()
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
            val body = JSONObject()
                .put("error", "proxy_failed")
                .put("message", e.message ?: e.javaClass.simpleName)
                .put("hint", "ensure the app's WebViewEvalServer is up on :28081")
                .toString()
            newFixedLengthResponse(Response.Status.INTERNAL_ERROR, "application/json", body)
        }
    }

    private fun readJsonBody(session: IHTTPSession): JSONObject {
        val files = HashMap<String, String>()
        session.parseBody(files)
        val payload = files["postData"] ?: session.queryParameterString ?: "{}"
        return JSONObject(payload)
    }

    private fun errorJson(status: Response.Status, kind: String, message: String): Response {
        val body = JSONObject()
            .put("error", kind)
            .put("message", message)
            .toString()
        return newFixedLengthResponse(status, "application/json", body)
    }

    private data class IntQuad(val a: Int, val b: Int, val c: Int, val d: Int)

    private fun serveNotImplemented(uri: String, method: String): Response {
        val body = JSONObject()
            .put("error", "not_implemented")
            .put("route", uri)
            .put("method", method)
            .toString()
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
            val id = short.takeIf { it.isNotEmpty() } ?: label.lowercase()
            val role = when {
                id.contains("cancel", ignoreCase = true) -> "cancel"
                id.contains("destruct", ignoreCase = true) ||
                    label.equals("Delete", ignoreCase = true) -> "destructive"
                else -> "default"
            }
            arr.put(
                JSONObject()
                    .put("id", id)
                    .put("label", label)
                    .put("role", role)
                    .put("destructive", role == "destructive"),
            )
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
        for (window in automation.windows) {
            val node = window.root ?: continue
            try {
                val obj = nodeToJson(node)
                rootChildren.put(obj)
                val bounds = Rect()
                node.getBoundsInScreen(bounds)
                if (bounds.right > maxW) maxW = bounds.right
                if (bounds.bottom > maxH) maxH = bounds.bottom
            } finally {
                node.recycle()
            }
        }
        return JSONObject()
            .put("rawType", "android.view.WindowRoot")
            .put("bounds", JSONObject().put("x", 0).put("y", 0).put("w", maxW).put("h", maxH))
            .put("enabled", true)
            .put("selected", false)
            .put("hasFocus", false)
            .put("visible", true)
            .put("children", rootChildren)
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
        val obj = JSONObject()
        val cls = node.className?.toString() ?: ""
        obj.put("rawType", cls)
        deriveRole(cls)?.let { obj.put("role", it) }

        node.viewIdResourceName?.takeIf { it.isNotEmpty() }?.let {
            // Match TreeXmlParser convention: strip "<pkg>:id/" prefix so
            // tests can match short id literally.
            val short = it.substringAfter(":id/", it)
            obj.put("identifier", short)
        }
        node.contentDescription?.toString()?.takeIf { it.isNotEmpty() }?.let {
            obj.put("label", it)
        }
        node.text?.toString()?.takeIf { it.isNotEmpty() }?.let {
            obj.put("text", it)
        }

        val rect = Rect()
        node.getBoundsInScreen(rect)
        obj.put(
            "bounds",
            JSONObject()
                .put("x", rect.left)
                .put("y", rect.top)
                .put("w", (rect.right - rect.left).coerceAtLeast(0))
                .put("h", (rect.bottom - rect.top).coerceAtLeast(0)),
        )

        obj.put("enabled", node.isEnabled)
        obj.put("selected", node.isSelected)
        obj.put("hasFocus", node.isFocused)
        obj.put("visible", node.isVisibleToUser || (rect.width() > 0 && rect.height() > 0))

        val childArr = JSONArray()
        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            try {
                childArr.put(nodeToJson(child))
            } finally {
                child.recycle()
            }
        }
        obj.put("children", childArr)
        return obj
    }

    private fun deriveRole(cls: String): String? {
        val tail = cls.substringAfterLast('.')
        return when {
            tail == "Button" || tail == "ImageButton" || tail.endsWith("Button") -> "button"
            tail == "EditText" || tail.endsWith("EditText") -> "textField"
            tail == "ImageView" -> "image"
            tail == "Switch" || tail == "SwitchCompat" -> "switch"
            tail == "CheckBox" -> "checkBox"
            tail == "RadioButton" -> "radio"
            tail == "TextView" || tail.endsWith("TextView") -> "staticText"
            tail.contains("RecyclerView") || tail.contains("ListView") -> "scrollView"
            tail.contains("TabLayout") || tail == "TabHost" -> "tabBar"
            tail.contains("Toolbar") || tail.contains("ActionBar") -> "navigationBar"
            tail == "WebView" -> "webView"
            else -> null
        }
    }
}

/// Map smix KeyName camelCase (per `smix_input::KeyName` serde rename) →
/// Android KeyEvent.KEYCODE_*. Returns null when no clean mapping.
object KeyMap {
    fun androidKeyCode(name: String): Int? = when (name) {
        "return" -> KeyEvent.KEYCODE_ENTER
        "delete" -> KeyEvent.KEYCODE_DEL
        "tab" -> KeyEvent.KEYCODE_TAB
        "space" -> KeyEvent.KEYCODE_SPACE
        "escape" -> KeyEvent.KEYCODE_ESCAPE
        "arrowUp" -> KeyEvent.KEYCODE_DPAD_UP
        "arrowDown" -> KeyEvent.KEYCODE_DPAD_DOWN
        "arrowLeft" -> KeyEvent.KEYCODE_DPAD_LEFT
        "arrowRight" -> KeyEvent.KEYCODE_DPAD_RIGHT
        else -> null
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
                        deriveRole(cls)?.let { obj.put("role", it) }

                        parser.getAttributeValue(null, "resource-id")
                            ?.takeIf { it.isNotEmpty() }
                            ?.let {
                                val short = it.substringAfter(":id/", it)
                                obj.put("identifier", short)
                            }
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

    /// Map android widget class → smix Role camelCase string. Returns null
    /// when no clean mapping (caller omits the role field). Matches the
    /// curated Role enum in smix-screen.
    private fun deriveRole(cls: String): String? {
        val tail = cls.substringAfterLast('.')
        return when {
            tail == "Button" || tail == "ImageButton" || tail.endsWith("Button") -> "button"
            tail == "EditText" || tail.endsWith("EditText") -> "textField"
            tail == "ImageView" -> "image"
            tail == "Switch" || tail == "SwitchCompat" -> "switch"
            tail == "CheckBox" -> "checkBox"
            tail == "RadioButton" -> "radio"
            tail == "TextView" || tail.endsWith("TextView") -> "staticText"
            tail.contains("RecyclerView") || tail.contains("ListView") -> "scrollView"
            tail.contains("TabLayout") || tail == "TabHost" -> "tabBar"
            tail.contains("Toolbar") || tail.contains("ActionBar") -> "navigationBar"
            tail == "WebView" -> "webView"
            else -> null
        }
    }
}
