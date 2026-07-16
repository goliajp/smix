// Selector full 7-case + Modifiers flatten + Pattern + fluent
// chaining roundtrip + Rust-compatible wire shape tests.
//
// Mirrors swift-bridge/Tests/SmixSDKTests/SelectorFullSchemaTests.swift.
// Verifies Pattern wire shape, Selector custom KSerializer untagged +
// flatten output, fluent chaining mutation semantics.

package dev.smix.sdk

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class SelectorFullSchemaTest {

    private val json = Json { encodeDefaults = true; prettyPrint = false }

    // MARK: - Pattern wire shape

    @Test
    fun patternLiteralEncodesAsBareString() {
        val encoded = json.encodeToString(PatternSerializer, Pattern.Literal("hello") as Pattern)
        assertEquals("\"hello\"", encoded)
    }

    @Test
    fun patternRegexEncodesAsObject() {
        val encoded = json.encodeToString(PatternSerializer, Pattern.Regex("foo.*", flags = "i") as Pattern)
        assertTrue("must contain regex field", encoded.contains("\"regex\":\"foo.*\""))
        assertTrue("must contain flags field", encoded.contains("\"flags\":\"i\""))
    }

    @Test
    fun patternRoundtripLiteral() {
        val original = Pattern.Literal("hello")
        val encoded = json.encodeToString(PatternSerializer, original as Pattern)
        val decoded = json.decodeFromString(PatternSerializer, encoded)
        assertEquals(original, decoded)
    }

    @Test
    fun patternRoundtripRegex() {
        val original = Pattern.Regex("^foo$", flags = "i")
        val encoded = json.encodeToString(PatternSerializer, original as Pattern)
        val decoded = json.decodeFromString(PatternSerializer, encoded)
        assertEquals(original, decoded)
    }

    // MARK: - Selector base wire shape (with modifier flatten)

    @Test
    fun selectorIdWithModifiersFlattens() {
        val sel = Selector.Id("btn-login")
            .below(Selector.Text(Pattern.Literal("Sign In")))
            .nth(0)
        val out = json.encodeToString(SelectorSerializer, sel)
        assertTrue("must have id", out.contains("\"id\":\"btn-login\""))
        assertTrue("must have below as nested Selector", out.contains("\"below\":{\"text\":\"Sign In\"}"))
        assertTrue("must have nth flattened", out.contains("\"nth\":0"))
    }

    @Test
    fun selectorRoleWithNamePattern() {
        val sel = Selector.Role("button", name = Pattern.Literal("Submit"))
        val out = json.encodeToString(SelectorSerializer, sel)
        assertTrue(out.contains("\"role\":\"button\""))
        assertTrue(out.contains("\"name\":\"Submit\""))
    }

    @Test
    fun selectorFocusedEncodesAsBoolean() {
        val out = json.encodeToString(SelectorSerializer, Selector.Focused as Selector)
        assertEquals("{\"focused\":true}", out)
    }

    @Test
    fun selectorAnchorWithIndex() {
        val sel = Selector.Anchor(
            AnchorBox(below = Selector.Text(Pattern.Literal("Address"))),
            index = IndexModifiers(nth = 2),
        )
        val out = json.encodeToString(SelectorSerializer, sel)
        assertTrue(out.contains("\"anchor\":{\"below\":{\"text\":\"Address\"}}"))
        assertTrue(out.contains("\"nth\":2"))
    }

    @Test
    fun selectorLocalizedText() {
        val sel = Selector.LocalizedText(mapOf("en" to "Submit", "ja" to "送信"))
        val out = json.encodeToString(SelectorSerializer, sel)
        assertTrue(out.contains("\"localizedText\""))
        assertTrue(out.contains("Submit"))
        assertTrue(out.contains("送信"))
    }

    // MARK: - Decode roundtrip

    @Test
    fun roundtripIdWithBelowAndNth() {
        val wire = """{"below":{"text":"Sign In"},"id":"btn-login","nth":0}"""
        val decoded = json.decodeFromString(SelectorSerializer, wire)
        assertTrue("decoded must be Id", decoded is Selector.Id)
        decoded as Selector.Id
        assertEquals("btn-login", decoded.id)
        assertEquals(0, decoded.modifiers.nth)
        assertEquals(Selector.Text(Pattern.Literal("Sign In")), decoded.modifiers.below)
    }

    @Test
    fun roundtripFocused() {
        val decoded = json.decodeFromString(SelectorSerializer, """{"focused":true}""")
        assertEquals(Selector.Focused, decoded)
    }

    @Test
    fun roundtripAnchor() {
        val wire = """{"anchor":{"below":{"text":"Address"}},"nth":2}"""
        val decoded = json.decodeFromString(SelectorSerializer, wire)
        assertTrue("decoded must be Anchor", decoded is Selector.Anchor)
        decoded as Selector.Anchor
        assertEquals(2, decoded.index.nth)
    }

    @Test
    fun roundtripLocalizedText() {
        val wire = """{"localizedText":{"en":"Submit","ja":"送信"}}"""
        val decoded = json.decodeFromString(SelectorSerializer, wire)
        assertTrue("decoded must be LocalizedText", decoded is Selector.LocalizedText)
        decoded as Selector.LocalizedText
        assertEquals("Submit", decoded.map["en"])
        assertEquals("送信", decoded.map["ja"])
    }

    @Test
    fun roundtripRolePatternRegex() {
        val wire = """{"name":{"flags":"i","regex":"^Sub"},"role":"button"}"""
        val decoded = json.decodeFromString(SelectorSerializer, wire)
        assertTrue("decoded must be Role", decoded is Selector.Role)
        decoded as Selector.Role
        assertEquals("button", decoded.role)
        assertEquals(Pattern.Regex("^Sub", flags = "i"), decoded.name)
    }

    // MARK: - Fluent chaining

    @Test
    fun fluentBelowSetsModifier() {
        val s = Selector.Id("btn").below(Selector.Text(Pattern.Literal("anchor")))
        assertTrue(s is Selector.Id)
        s as Selector.Id
        assertEquals(Selector.Text(Pattern.Literal("anchor")), s.modifiers.below)
    }

    @Test
    fun fluentChainedMultiple() {
        val s = Selector.Id("btn")
            .below(Selector.Text(Pattern.Literal("address")))
            .above(Selector.Text(Pattern.Literal("title")))
            .nth(3)
        assertTrue(s is Selector.Id)
        s as Selector.Id
        assertNotNull(s.modifiers.below)
        assertNotNull(s.modifiers.above)
        assertEquals(3, s.modifiers.nth)
    }

    @Test
    fun fluentFirstLast() {
        val f = Selector.Label("Item").first()
        val l = Selector.Label("Item").last()
        assertEquals(true, (f as Selector.Label).modifiers.first)
        assertEquals(true, (l as Selector.Label).modifiers.last)
    }

    @Test
    fun fluentNearAncestor() {
        val s = Selector.Text(Pattern.Literal("Cancel"))
            .near(Selector.Text(Pattern.Literal("Confirm")))
            .ancestor(Selector.Role("dialog"))
        assertTrue(s is Selector.Text)
        s as Selector.Text
        assertNotNull(s.modifiers.near)
        assertNotNull(s.modifiers.ancestor)
    }

    // MARK: - Roundtrip with modifiers

    @Test
    fun roundtripIdWithMultipleModifiers() {
        val original = Selector.Id("btn-foo")
            .below(Selector.Text(Pattern.Literal("Address")))
            .nth(0)
        val encoded = json.encodeToString(SelectorSerializer, original)
        val decoded = json.decodeFromString(SelectorSerializer, encoded)
        assertEquals(original, decoded)
    }

    @Test
    fun roundtripRoleWithModifiersAndName() {
        val original = Selector.Role("button", name = Pattern.Literal("Submit"))
            .below(Selector.Text(Pattern.Literal("Email")))
        val encoded = json.encodeToString(SelectorSerializer, original)
        val decoded = json.decodeFromString(SelectorSerializer, encoded)
        assertEquals(original, decoded)
    }
}
