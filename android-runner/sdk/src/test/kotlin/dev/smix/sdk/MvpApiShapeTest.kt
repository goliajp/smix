// Kotlin SDK MVP shape JVM unit tests.
//
// Mirrors swift-bridge/Tests/SmixSDKTests/MvpApiShapeTests.swift.
// Verifies:
//   - Selector 4 base case constructible
//   - Modifier 9 case constructible
//   - A11yRole 28 case exposed
//   - A11yNode roundtrip + camelCase wire keys
//   - FailureCode 6 case
//   - ExpectationFailure errorJson() AI-readable contract
//   - Smix.launchApp / App.tap / Locator.toBeVisible stubs throw NotImplemented

package dev.smix.sdk

import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class MvpApiShapeTest {

    // MARK: - Selector cases

    @Test
    fun selectorBaseCasesExhaustive() {
        // Text/Role.name take a Pattern, not a String.
        val cases: List<Selector> = listOf(
            Selector.Id("btn-login"),
            Selector.Text(Pattern.Literal("Sign In")),
            Selector.Label("Settings"),
            Selector.Role("button"),
            Selector.Role("button", name = Pattern.Literal("Submit")),
        )
        assertEquals(5, cases.size)
    }

    @Test
    fun selectorIdSerializesAsExpectedShape() {
        val sel = Selector.Id("btn-login")
        val json = Json.encodeToString(SelectorSerializer, sel)
        assertTrue(json.contains("\"id\":\"btn-login\""))
    }

    // MARK: - Role cases

    @Test
    fun a11yRoleCasesAllExposed() {
        // Snapshot mirror of the Rust smix-screen Role enum.
        val expected = setOf(
            "BUTTON", "LINK", "TEXT_FIELD", "SECURE_TEXT_FIELD", "SEARCH_FIELD",
            "SWITCH", "TOGGLE", "CHECK_BOX", "RADIO", "IMAGE", "STATIC_TEXT",
            "TAB", "TAB_BAR", "NAVIGATION_BAR", "CELL", "ALERT", "DIALOG",
            "SLIDER", "PROGRESS_BAR", "PICKER", "MENU", "MENU_ITEM",
            "SCROLL_VIEW", "SEGMENTED_CONTROL", "TABLE", "COLLECTION_VIEW",
            "WEB_VIEW", "KEYBOARD",
        )
        val actual = A11yRole.entries.map { it.name }.toSet()
        assertEquals(expected, actual)
        assertEquals(28, A11yRole.entries.size)
    }

    @Test
    fun a11yRoleSerializesAsCamelCase() {
        val json = Json.encodeToString(A11yRole.serializer(), A11yRole.BUTTON)
        assertEquals("\"button\"", json)

        val jsonScrollView = Json.encodeToString(A11yRole.serializer(), A11yRole.SCROLL_VIEW)
        assertEquals("\"scrollView\"", jsonScrollView)
    }

    // MARK: - A11yNode + Rect shape

    @Test
    fun a11yNodeRoundtripMinimal() {
        val node = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 393.0, 852.0),
            enabled = true,
            visible = true,
        )
        val encoded = Json.encodeToString(A11yNode.serializer(), node)
        val decoded = Json.decodeFromString(A11yNode.serializer(), encoded)
        assertEquals("other", decoded.rawType)
        assertEquals(393.0, decoded.bounds.w, 0.001)
    }

    @Test
    fun a11yNodeUsesCamelCaseKeys() {
        val node = A11yNode(
            rawType = "other",
            bounds = Rect(0.0, 0.0, 1.0, 1.0),
            hasFocus = true,
            visible = true,
        )
        val json = Json.encodeToString(A11yNode.serializer(), node)
        assertTrue("rawType camelCase", json.contains("\"rawType\":\"other\""))
        assertTrue("hasFocus camelCase", json.contains("\"hasFocus\":true"))
    }

    @Test
    fun rectCenterComputed() {
        val r = Rect(100.0, 200.0, 80.0, 40.0)
        assertEquals(140.0, r.centerX, 0.001)
        assertEquals(220.0, r.centerY, 0.001)
    }

    // MARK: - FailureCode

    @Test
    fun failureCodeCasesAllExposed() {
        val expected = setOf(
            "NOT_FOUND", "AMBIGUOUS", "NOT_INTERACTABLE",
            "TIMEOUT", "WRONG_STATE", "UNKNOWN",
        )
        val actual = FailureCode.entries.map { it.name }.toSet()
        assertEquals(expected, actual)
        assertEquals(6, FailureCode.entries.size)
    }

    @Test
    fun failureCodeSerializesAsCamelCase() {
        assertEquals("\"notFound\"", Json.encodeToString(FailureCode.serializer(), FailureCode.NOT_FOUND))
        assertEquals("\"notInteractable\"", Json.encodeToString(FailureCode.serializer(), FailureCode.NOT_INTERACTABLE))
    }

    // MARK: - ExpectationFailure

    @Test
    fun expectationFailureErrorJsonHasStableKeys() {
        val failure = ExpectationFailure(
            code = FailureCode.NOT_FOUND,
            message = "no candidates",
            selectorJson = "{\"id\":\"btn\"}",
            suggestions = listOf("check id"),
            timestamp = 1_780_000_000L,
        )
        val json = failure.errorJson()
        // Each key appears exactly once
        for (key in listOf("code", "message", "selector", "visibleElements", "suggestions", "timestamp")) {
            val count = json.split("\"$key\":").size - 1
            assertEquals("key '$key' should appear exactly once in: $json", 1, count)
        }
    }

    @Test
    fun expectationFailureErrorJsonContainsCamelCaseFailureCode() {
        val failure = ExpectationFailure(
            code = FailureCode.WRONG_STATE,
            message = "x",
        )
        val json = failure.errorJson()
        assertTrue("must contain wrongState rawValue", json.contains("\"wrongState\""))
    }

    // No stubbed surface remains — every act/sense method is wired
    // through the FFI driving seam (Driver / Session). Behavioural
    // coverage lives in the seam-injected suites: AppTapMockTest,
    // LocatorMockTest, AppActSenseExtMockTest, LocatorToHaveMockTest.
}
