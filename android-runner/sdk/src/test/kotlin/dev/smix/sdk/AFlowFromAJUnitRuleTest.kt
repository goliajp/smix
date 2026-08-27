package dev.smix.sdk

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The same three facts the other two readers take out of the same bytes. */
class AFlowFromAJUnitRuleTest {

    private fun payload(name: String): String =
        checkNotNull(javaClass.getResourceAsStream("/reports/$name.xml")) {
            "the recorded $name payload is missing from the test resources"
        }.bufferedReader().readText()

    @Test
    fun `a passing run names the flow`() {
        val r = SmixFlow.parse(payload("passing"))
        assertEquals("dialog-confirm", r.flow)
        assertTrue(r.passed)
        assertNull(r.failure)
    }

    @Test
    fun `a failing run carries the step the verb and the reason`() {
        val r = SmixFlow.parse(payload("failing"))
        assertFalse(r.passed)
        val f = checkNotNull(r.failure)
        assertTrue("the reason does not say which step: $f", f.contains("step 2"))
        assertTrue("the reason does not name the verb: $f", f.contains("tapOn"))
        assertTrue("the reason lost the selector: $f", f.contains("no-such-control"))
    }

    @Test
    fun `nothing is not a pass`() {
        for (input in listOf("", "total nonsense")) {
            try {
                SmixFlow.parse(input)
                throw AssertionError("`$input` was read as a report")
            } catch (e: FlowReportError.NotAReport) {
                // expected
            }
        }
    }

    @Test
    fun `a suite with no case is not a pass either`() {
        val empty = """
            <?xml version="1.0" encoding="UTF-8"?>
            <testsuite name="smix" tests="0" failures="0" errors="0" skipped="0">
            </testsuite>
        """.trimIndent()
        try {
            SmixFlow.parse(empty)
            throw AssertionError("a suite with no testcase was read as a pass")
        } catch (e: FlowReportError.NoFlowInIt) {
            // expected
        }
    }

    @Test
    fun `the attribute path is read and unescaped`() {
        // Both recorded payloads carry CDATA, where nothing is escaped, so
        // neither the attribute fallback nor the unescaping runs against
        // them. A writer that drops CDATA leaves only the attribute, and
        // there everything IS escaped.
        val attributeOnly = """
            <?xml version="1.0" encoding="UTF-8"?>
            <testsuite name="smix" tests="1" failures="1" errors="0" skipped="0">
              <testcase name="attr-only" classname="smix.flow" time="0">
                  <failure type="smix.sdk" message="step 2 (tapOn): not found: { id=&quot;x&quot; }"/>
              </testcase>
            </testsuite>
        """.trimIndent()
        val r = SmixFlow.parse(attributeOnly)
        assertFalse(r.passed)
        val f = checkNotNull(r.failure)
        assertTrue("the attribute path lost the step: $f", f.contains("step 2"))
        assertTrue("the attribute path did not unescape: $f", f.contains("id=\"x\""))
    }
}
