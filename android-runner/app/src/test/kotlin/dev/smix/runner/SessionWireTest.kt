// Decode + encode coverage for the /session/* routes in
// SmixHttpServer.serve. Assertions parse the emitted string back through
// JSONObject — key order is not part of the wire contract.
//
// Contract cross-checks (crates/smix-runner-wire/src/lib.rs, all
// camelCase via serde rename_all):
// - /session/open            → SessionOpenRequest {bundleId, activate}
//                              / SessionOpenResponse {sessionId,
//                              activatedOnce, serverTimeMs}
// - /session/close           → SessionCloseRequest {sessionId}
//                              / SessionCloseResponse {ok} (idempotent)
// - /session/close-all       → SessionCloseAllResponse {ok, closed}
// - /session/renew-activation → SessionRenewActivationRequest
//                              {sessionId} / {ok, activated}
// - /session/list            → SessionListResponse {sessions:
//                              [SessionSummary {sessionId, bundleId,
//                              openedAtMs, lastActivatedAtMs,
//                              interactiveNamedIds}]}
// - /session/launch-app + /session/terminate-app
//                            → SessionAppLifecycleRequest {sessionId,
//                              args, env, waitForForegroundMs,
//                              waitForInteractiveMs} /
//                              SessionAppLifecycleResponse {ok, wallMs,
//                              waitedMs, terminalState,
//                              terminatedCooperatively,
//                              reachedInteractive, interactiveNamedIds}
// - /session/relaunch-app    → SessionRelaunchAppRequest {sessionId}
//                              / SessionRelaunchAppResponse {ok, wallMs}
// - unknown session          → iOS SessionRoute.notFound envelope
//                              {ok:false, error:"not_found", reason}

package dev.smix.runner

import org.json.JSONException
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class SessionWireTest {

    // MARK: - /session/open decode

    @Test
    fun sessionOpenDecodesBundleIdAndActivate() {
        val req = RunnerWire.decodeSessionOpen(
            """{"bundleId":"com.example.app","activate":true}""",
        )
        assertEquals("com.example.app", req.bundleId)
        assertTrue(req.activate)
    }

    @Test
    fun sessionOpenActivateDefaultsFalse() {
        val req = RunnerWire.decodeSessionOpen("""{"bundleId":"com.example.app"}""")
        assertFalse(req.activate)
    }

    @Test
    fun sessionOpenMissingBundleIdDecodesEmpty() {
        // serde(default) on the Rust side — decode never throws on an
        // absent key; the handler maps "" to the 400 bad_request
        // envelope.
        val req = RunnerWire.decodeSessionOpen("""{"activate":true}""")
        assertEquals("", req.bundleId)
    }

    @Test
    fun sessionOpenNonJsonThrows() {
        assertThrows(JSONException::class.java) {
            RunnerWire.decodeSessionOpen("not json")
        }
    }

    // MARK: - sessionId decode (close / renew / lifecycle / relaunch)

    @Test
    fun sessionIdDecodes() {
        assertEquals(
            "sess-android-1",
            RunnerWire.decodeSessionId("""{"sessionId":"sess-android-1"}"""),
        )
    }

    @Test
    fun sessionIdMissingDecodesEmpty() {
        assertEquals("", RunnerWire.decodeSessionId("{}"))
    }

    @Test
    fun sessionIdIgnoresLifecycleExtras() {
        // The Rust SessionAppLifecycleRequest sends args/env/waitFor*Ms
        // alongside sessionId; Android reads only the id.
        val payload = """
            {"sessionId":"s1","args":["-flag"],"env":{"K":"V"},
             "waitForForegroundMs":15000,"waitForInteractiveMs":30000}
        """.trimIndent()
        assertEquals("s1", RunnerWire.decodeSessionId(payload))
    }

    @Test
    fun sessionIdNonJsonThrows() {
        assertThrows(JSONException::class.java) {
            RunnerWire.decodeSessionId("not json")
        }
    }

    // MARK: - /session/open encode

    @Test
    fun sessionOpenBodyShape() {
        val obj = JSONObject(RunnerWire.sessionOpenBody("sess-android-1", false, 1234L))
        assertTrue(obj.getBoolean("ok"))
        assertEquals("sess-android-1", obj.getString("sessionId"))
        assertFalse(obj.getBoolean("activatedOnce"))
        assertEquals(1234L, obj.getLong("serverTimeMs"))
    }

    @Test
    fun sessionOpenBodyEchoesActivatedOnce() {
        val obj = JSONObject(RunnerWire.sessionOpenBody("sess-android-2", true, 5L))
        assertTrue(obj.getBoolean("activatedOnce"))
    }

    // MARK: - /session/close + /session/close-all encode

    @Test
    fun sessionCloseBodyIsBareOkTrue() {
        val obj = JSONObject(RunnerWire.sessionCloseBody())
        assertTrue(obj.getBoolean("ok"))
        assertEquals(1, obj.length())
    }

    @Test
    fun sessionCloseAllBodyCarriesCount() {
        val obj = JSONObject(RunnerWire.sessionCloseAllBody(3))
        assertTrue(obj.getBoolean("ok"))
        assertEquals(3, obj.getInt("closed"))
    }

    // MARK: - /session/renew-activation encode

    @Test
    fun sessionRenewBodyShape() {
        val obj = JSONObject(RunnerWire.sessionRenewBody(activated = true))
        assertTrue(obj.getBoolean("ok"))
        assertTrue(obj.getBoolean("activated"))
    }

    // MARK: - /session/list encode

    @Test
    fun sessionListBodyMatchesSessionSummary() {
        val table = SessionTable(nowMs = { 100L })
        table.open("com.example.app", activated = true)
        val obj = JSONObject(RunnerWire.sessionListBody(table.list()))
        val arr = obj.getJSONArray("sessions")
        assertEquals(1, arr.length())
        val entry = arr.getJSONObject(0)
        assertEquals("sess-android-1", entry.getString("sessionId"))
        assertEquals("com.example.app", entry.getString("bundleId"))
        assertEquals(100L, entry.getLong("openedAtMs"))
        assertEquals(100L, entry.getLong("lastActivatedAtMs"))
        assertEquals(0, entry.getJSONArray("interactiveNamedIds").length())
    }

    @Test
    fun sessionListBodyEmptyTable() {
        val obj = JSONObject(RunnerWire.sessionListBody(emptyList()))
        assertEquals(0, obj.getJSONArray("sessions").length())
    }

    // MARK: - /session/launch-app + /session/terminate-app encode

    @Test
    fun sessionLifecycleBodyMatchesRustStruct() {
        val obj = JSONObject(RunnerWire.sessionLifecycleBody(true, 150L))
        assertTrue(obj.getBoolean("ok"))
        assertEquals(150L, obj.getLong("wallMs"))
        assertEquals(0L, obj.getLong("waitedMs"))
        assertEquals(0, obj.getInt("terminalState"))
        assertFalse(obj.getBoolean("terminatedCooperatively"))
        assertFalse(obj.getBoolean("reachedInteractive"))
        assertEquals(0, obj.getJSONArray("interactiveNamedIds").length())
    }

    // MARK: - /session/relaunch-app encode

    @Test
    fun sessionRelaunchBodyShape() {
        val obj = JSONObject(RunnerWire.sessionRelaunchBody(true, 88L))
        assertTrue(obj.getBoolean("ok"))
        assertEquals(88L, obj.getLong("wallMs"))
    }

    // MARK: - error envelopes

    @Test
    fun sessionNotFoundEnvelopeMatchesIos() {
        val obj = JSONObject(RunnerWire.sessionNotFoundBody("unknown session id"))
        assertFalse(obj.getBoolean("ok"))
        assertEquals("not_found", obj.getString("error"))
        assertEquals("unknown session id", obj.getString("reason"))
    }

    @Test
    fun sessionBadRequestEnvelopeMatchesIos() {
        val obj = JSONObject(RunnerWire.sessionBadRequestBody("bundleId must be a non-empty string"))
        assertFalse(obj.getBoolean("ok"))
        assertEquals("bad_request", obj.getString("error"))
        assertEquals("bundleId must be a non-empty string", obj.getString("reason"))
    }

    // MARK: - shell commands

    @Test
    fun terminateAppCommandIsForceStop() {
        assertEquals(
            "am force-stop com.example.app",
            RunnerWire.terminateAppCommand("com.example.app"),
        )
    }
}
