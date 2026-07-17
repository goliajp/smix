// App.tap orchestration unit tests over the driving seam.
//
// Verifies App logic: driver.tree() → SelectorResolver → session.tapById,
// plus the .notFound / resolver-error failure shapes. Injects in-memory
// MockDriver/MockSession — JVM-only, no JNA init, no libuniffi_smix.so.
// Driving-wire correctness is the C8 Rust wiremock's job.

package dev.smix.sdk

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class AppTapMockTest {

    // MARK: - .notFound path

    @Test
    fun tapByIdEmptyTreeThrowsNotFound() = runBlocking {
        val app = mockApp(resolver = MockSelectorResolver())  // returns [] for everything
        try {
            app.tap(Selector.Id("btn-missing"))
            fail("empty resolver result must yield NOT_FOUND")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.NOT_FOUND, e.code)
            assertFalse("suggestions must populate for AI agent", e.suggestions.isEmpty())
            assertFalse(e.visibleElements.isEmpty())
        }
    }

    @Test
    fun tapByTextAbsentThrowsNotFound() = runBlocking {
        val app = mockApp(resolver = MockSelectorResolver())
        try {
            app.tap(Selector.Text(Pattern.Literal("Nope")))
            fail("text miss must yield NOT_FOUND")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.NOT_FOUND, e.code)
        }
    }

    // MARK: - Hit path

    @Test
    fun tapByIdHitTapsById() = runBlocking {
        val button = A11yNode(
            rawType = "button",
            role = A11yRole.BUTTON,
            identifier = "btn-login",
            label = "Sign In",
            bounds = Rect(100.0, 200.0, 80.0, 40.0),
            enabled = true,
            visible = true,
        )
        val root = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 393.0, 852.0),
            enabled = true,
            visible = true,
            children = listOf(button),
        )
        val session = MockSession()
        val resolver = MockSelectorResolver().apply {
            registerHit("""{"id":"btn-login"}""", "btn-login")
        }
        val app = mockApp(tree = root, session = session, resolver = resolver)

        app.tap(Selector.Id("btn-login"))

        assertEquals("resolved id tapped exactly once", listOf("btn-login"), session.tapByIdCalls)
    }

    @Test
    fun tapByLabelHitTapsById() = runBlocking {
        val button = A11yNode(
            rawType = "button",
            role = A11yRole.BUTTON,
            identifier = "btn-foo",
            label = "Submit",
            bounds = Rect(50.0, 600.0, 100.0, 50.0),
            enabled = true,
            visible = true,
        )
        val root = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 393.0, 852.0),
            enabled = true,
            visible = true,
            children = listOf(button),
        )
        val session = MockSession()
        val resolver = MockSelectorResolver().apply {
            registerHit("""{"label":"Submit"}""", "btn-foo")
        }
        val app = mockApp(tree = root, session = session, resolver = resolver)

        app.tap(Selector.Label("Submit"))
        assertEquals(listOf("btn-foo"), session.tapByIdCalls)
    }

    @Test
    fun tapPassesEncodedSelectorJsonToResolver() = runBlocking {
        val resolver = MockSelectorResolver()  // returns []
        val app = mockApp(resolver = resolver)

        try {
            app.tap(Selector.Id("btn-x"))
        } catch (_: ExpectationFailure) { /* expected */ }

        assertEquals(1, resolver.calls.size)
        val (_, sel) = resolver.calls[0]
        // selectorJson must equal what encodeSelectorJson produces — the
        // Rust-compatible untagged shape `{"id":"btn-x"}`.
        assertEquals("""{"id":"btn-x"}""", sel)
    }

    @Test
    fun tapResolverErrorYieldsUnknownCode() = runBlocking {
        val resolver = MockSelectorResolver().apply {
            throwOnNext = RuntimeException("simulated FFI parse error")
        }
        val app = mockApp(resolver = resolver)

        try {
            app.tap(Selector.Id("anything"))
            fail("resolver raise must surface as ExpectationFailure")
        } catch (e: ExpectationFailure) {
            assertEquals(FailureCode.UNKNOWN, e.code)
            assertTrue("message must include resolver error", e.message.contains("simulated FFI parse error"))
        }
    }

    // MARK: - Lifecycle

    @Test
    fun relaunchDelegatesToSession() = runBlocking {
        val session = MockSession()
        val app = mockApp(session = session)
        app.relaunch()
        assertEquals(1, session.relaunchAppCalls)
    }

    @Test
    fun terminateDelegatesToSession() = runBlocking {
        val session = MockSession()
        val app = mockApp(session = session)
        app.terminate()
        assertEquals(1, session.terminateAppCalls)
    }

    // MARK: - tree() sense

    @Test
    fun treeReturnsDecodedSnapshotFromDriver() = runBlocking {
        val snapshot = A11yNode(
            rawType = "other",
            identifier = "root",
            bounds = Rect(0.0, 0.0, 1.0, 1.0),
            visible = true,
            children = emptyList(),
        )
        val app = mockApp(tree = snapshot)
        val tree = app.tree()
        assertEquals(snapshot, tree)
    }
}
