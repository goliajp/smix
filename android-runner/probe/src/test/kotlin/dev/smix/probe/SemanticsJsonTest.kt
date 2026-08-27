package dev.smix.probe

import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The wire carries what the accessibility projection loses.
 *
 * Every field asserted here is one the a11y path either drops or gets
 * wrong on Compose, which is the whole reason this module exists. The
 * payload mirrors the fixture's Compose screen.
 */
class SemanticsJsonTest {
    private fun node(
        id: Int,
        tag: String? = null,
        text: String? = null,
        editable: String? = null,
        focused: Boolean = false,
        actions: List<String> = emptyList(),
        children: List<ProbeNode> = emptyList(),
    ) = ProbeNode(
        id = id,
        testTag = tag,
        text = text,
        editableText = editable,
        inputText = editable,
        contentDescription = null,
        role = null,
        bounds = Bounds(0, 0, 100, 40),
        focused = focused,
        enabled = true,
        actions = actions,
        children = children,
    )

    private val screen = listOf(
        node(
            1,
            children = listOf(
                node(2, tag = "compose_input", editable = "hello", focused = true,
                     actions = listOf("SetText", "OnClick")),
                // The masked one. a11y reports "••••••" here and the real
                // characters nowhere.
                node(3, tag = "compose_secret", editable = "s3cret"),
                node(4, tag = "compose_submit", text = "Submit", actions = listOf("OnClick")),
            ),
        ),
    )

    @Test
    fun `carries the test tag without the app opting in`() {
        assertTrue(
            "the tree does not name compose_input",
            screen.toWireJson().contains("\"testTag\":\"compose_input\""),
        )
    }

    @Test
    fun `carries what a masked field actually holds`() {
        assertTrue(
            "the masked field's real text is not on the wire",
            screen.toWireJson().contains("\"editableText\":\"s3cret\""),
        )
    }

    @Test
    fun `says which node holds focus`() {
        val json = screen.toWireJson()
        val focusedAt = json.indexOf("\"focused\":true")
        assertTrue("no node reports focus", focusedAt >= 0)
        assertTrue(
            "focus is not reported on compose_input",
            json.lastIndexOf("compose_input", focusedAt) >= 0 &&
                json.indexOf("compose_secret").let { it < 0 || it > focusedAt },
        )
    }

    @Test
    fun `carries the actions a node will accept`() {
        assertTrue(
            "SetText is not on the wire, so nothing can tell an editable node from a label",
            screen.toWireJson().contains("SetText"),
        )
    }
}

/**
 * What a user typed goes on the wire verbatim, and a quote in it is not a
 * new field.
 *
 * Separate from the tests above because those ask what the wire carries
 * and this asks whether the wire survives it. A field that silently ends
 * early reads, downstream, exactly like a field the toolkit never had.
 */
class WireEscapingTest {
    private fun typed(text: String) = listOf(
        ProbeNode(
            id = 1, testTag = "compose_input", text = null, editableText = text, inputText = text,
            contentDescription = null, role = null, bounds = Bounds(0, 0, 1, 1),
            focused = false, enabled = true, actions = emptyList(), children = emptyList(),
        ),
    )

    @Test
    fun `a quote in the typed text does not end the field`() {
        val json = typed("say \"hi\"").toWireJson()
        assertTrue("the quote was not escaped: $json", json.contains("""\"hi\""""))
        assertTrue("the node lost its shape after the quote", json.trimEnd().endsWith("}]"))
    }

    @Test
    fun `a newline in the typed text stays inside the string`() {
        val json = typed("one\ntwo").toWireJson()
        assertTrue("the newline was written raw: $json", !json.contains('\n'))
        assertTrue("the newline was not escaped", json.contains("""one\ntwo"""))
    }

    @Test
    fun `a backslash is not an escape for whatever follows it`() {
        val json = typed("""C:\temp""").toWireJson()
        assertTrue("the backslash was not doubled: $json", json.contains("""C:\\temp"""))
    }
}
