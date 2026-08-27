package dev.smix.sdk

/**
 * What a smix run tells a host test framework.
 *
 * Native teams live in Gradle. smix walks in rather than asking them out: a
 * flow runs through the same CLI that CI runs, and its JUnit report becomes
 * whatever JUnit calls a failure.
 *
 * Third of three readers of one document — the Rust one sits beside the
 * writer, and a Swift one serves XCTest. What keeps them honest is that all
 * three run against the same recorded payloads, so a shape that drifts
 * breaks all of them at once rather than one of them quietly.
 */
data class FlowReport(
    /** The flow's name, as the report names it. */
    val flow: String,
    /** Whether it passed. */
    val passed: Boolean,
    /** Why not. `null` exactly when [passed]. */
    val failure: String?,
)

/**
 * Why a report could not be read.
 *
 * Distinct from "it failed". A run that never happened and a run that failed
 * want different things from a caller, and one value for both is how an
 * empty string becomes a green test.
 */
sealed class FlowReportError(message: String) : Exception(message) {
    /** Not a smix report at all — usually the CLI never ran. */
    object NotAReport : FlowReportError("this is not a smix report — did the CLI run?")

    /** A report, with no flow in it. */
    object NoFlowInIt : FlowReportError("the report names no flow")
}

object SmixFlow {
    /** Parse the JUnit XML `smix run --format junit` writes. */
    @JvmStatic
    fun parse(junitXml: String): FlowReport {
        if (!junitXml.contains("<testsuite")) throw FlowReportError.NotAReport
        val flow = attribute(junitXml, "<testcase", "name") ?: throw FlowReportError.NoFlowInIt
        val raw = between(junitXml, "<![CDATA[", "]]>")
            ?: attribute(junitXml, "<failure", "message")
        val failure = raw?.let(::unescape)
        return FlowReport(flow = flow, passed = failure == null, failure = failure)
    }

    private fun attribute(xml: String, tag: String, name: String): String? {
        val start = xml.indexOf(tag).takeIf { it >= 0 } ?: return null
        val end = xml.indexOf('>', start).takeIf { it >= 0 } ?: return null
        val head = xml.substring(start, end)
        val key = "$name=\""
        val at = head.indexOf(key).takeIf { it >= 0 }?.plus(key.length) ?: return null
        val close = head.indexOf('"', at).takeIf { it >= 0 } ?: return null
        return head.substring(at, close)
    }

    private fun between(xml: String, open: String, close: String): String? {
        val a = xml.indexOf(open).takeIf { it >= 0 }?.plus(open.length) ?: return null
        val b = xml.indexOf(close, a).takeIf { it >= 0 } ?: return null
        return xml.substring(a, b)
    }

    /**
     * Undo the escaping the writer applies.
     *
     * A reader that hands `&quot;` to a developer has made the failure
     * harder to read than the stdout it replaced.
     */
    private fun unescape(s: String): String = s
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}
