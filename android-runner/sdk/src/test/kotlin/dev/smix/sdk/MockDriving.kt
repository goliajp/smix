// JVM test seams: in-memory Driver + Session mocks and a mockApp()
// builder. App-orchestration tests inject these through the driving
// seam (Session.kt) so they exercise App logic (resolve → tapById,
// failure shapes) without loading the Android-only libuniffi_smix.so.
// Driving-wire correctness is the C8 Rust wiremock's job, not these.

package dev.smix.sdk

import kotlinx.serialization.json.Json

/** In-memory [Driver]: returns a canned tree as JSON, records nothing else. */
class MockDriver(
    var treeResult: A11yNode = emptyTree(),
) : Driver {
    var listSessionsJson: String = """{"sessions":[]}"""
    var treeCallCount: Int = 0
        private set

    override suspend fun tree(): String {
        treeCallCount++
        return Json.encodeToString(A11yNode.serializer(), treeResult)
    }

    override suspend fun openSession(bundleId: String): Session =
        throw UnsupportedOperationException("MockDriver.openSession is not used in host tests")

    override suspend fun listSessions(): String = listSessionsJson

    companion object {
        fun emptyTree(): A11yNode = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 393.0, 852.0),
            enabled = true,
            visible = true,
            children = emptyList(),
        )
    }
}

/** In-memory [Session]: records every act call for assertions. */
class MockSession(
    var idResult: String = "mock-session",
    var tapByIdResult: Boolean = true,
    var systemPopupsJson: String = "[]",
) : Session {
    val tapByIdCalls = mutableListOf<String>()
    val tapAtNormCoordCalls = mutableListOf<Pair<Double, Double>>()
    val inputTextCalls = mutableListOf<String>()
    val pressKeyCalls = mutableListOf<String>()
    val swipeOnceCalls = mutableListOf<String>()
    var launchAppCalls = 0
        private set
    var terminateAppCalls = 0
        private set
    var relaunchAppCalls = 0
        private set
    var renewActivationCalls = 0
        private set
    var closeCalls = 0
        private set

    override fun id(): String = idResult
    override suspend fun launchApp() { launchAppCalls++ }
    override suspend fun tapById(id: String): Boolean {
        tapByIdCalls.add(id)
        return tapByIdResult
    }
    override suspend fun tapAtNormCoord(nx: Double, ny: Double) { tapAtNormCoordCalls.add(nx to ny) }
    override suspend fun inputText(text: String) { inputTextCalls.add(text) }
    override suspend fun pressKey(key: String) { pressKeyCalls.add(key) }
    override suspend fun swipeOnce(direction: String) { swipeOnceCalls.add(direction) }
    override suspend fun systemPopups(): String = systemPopupsJson
    override suspend fun terminateApp() { terminateAppCalls++ }
    override suspend fun relaunchApp() { relaunchAppCalls++ }
    override suspend fun renewActivation() { renewActivationCalls++ }
    override suspend fun close() { closeCalls++ }
}

/**
 * Build an [App] directly on the mock seams. Bypasses [Smix.launchApp],
 * which constructs the UniFFI-backed FfiDriver and would crash on host.
 */
fun mockApp(
    tree: A11yNode = MockDriver.emptyTree(),
    driver: MockDriver = MockDriver(treeResult = tree),
    session: MockSession = MockSession(),
    resolver: SelectorResolver = MockSelectorResolver(),
    labelsResolver: LabelResolver = MockLabelsResolver(),
): App = App(
    bundleId = "dev.smix.fixture",
    driver = driver,
    session = session,
    resolver = resolver,
    labelsResolver = labelsResolver,
)
