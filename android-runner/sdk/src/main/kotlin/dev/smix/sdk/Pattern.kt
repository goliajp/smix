// v7.4 c4 — Pattern (literal vs regex) mirror Swift v7.2 c3 + Rust
// smix-selector `#[serde(untagged)] enum Pattern`.
//
// Wire JSON forms (untagged):
//   Literal: bare string             e.g. "hello"
//   Regex:   {"regex":"...", "flags":"i"}  (flags default "i")

package dev.smix.sdk

import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.descriptors.buildClassSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonEncoder
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/** A text-match pattern — literal (strict equal) or regex. */
@Serializable(with = PatternSerializer::class)
sealed interface Pattern {
    /** Strict literal match (case-sensitive). */
    data class Literal(val value: String) : Pattern

    /**
     * Regex match. Flags default to "i" (case-insensitive) to mirror
     * Rust smix-selector v1.5 c5i-d maestro parity.
     */
    data class Regex(val regex: String, val flags: String = "i") : Pattern
}

/**
 * Custom KSerializer for [Pattern] — Rust untagged shape via
 * JsonContentPolymorphicSerializer-style discrimination at the JSON
 * tree level. Falls back to error if discriminator missing on decode.
 */
object PatternSerializer : KSerializer<Pattern> {
    override val descriptor: SerialDescriptor =
        buildClassSerialDescriptor("dev.smix.sdk.Pattern")

    override fun serialize(encoder: Encoder, value: Pattern) {
        val jsonEncoder = encoder as? JsonEncoder
            ?: error("PatternSerializer requires JSON encoder")
        when (value) {
            is Pattern.Literal -> jsonEncoder.encodeJsonElement(JsonPrimitive(value.value))
            is Pattern.Regex -> {
                val obj = JsonObject(
                    mapOf(
                        "regex" to JsonPrimitive(value.regex),
                        "flags" to JsonPrimitive(value.flags),
                    )
                )
                jsonEncoder.encodeJsonElement(obj)
            }
        }
    }

    override fun deserialize(decoder: Decoder): Pattern {
        val jsonDecoder = decoder as? JsonDecoder
            ?: error("PatternSerializer requires JSON decoder")
        return when (val tree = jsonDecoder.decodeJsonElement()) {
            is JsonPrimitive -> Pattern.Literal(tree.content)
            is JsonObject -> {
                val regex = tree["regex"]?.jsonPrimitive?.contentOrNull
                    ?: error("Pattern object missing 'regex' field: $tree")
                val flags = tree["flags"]?.jsonPrimitive?.contentOrNull ?: "i"
                Pattern.Regex(regex, flags)
            }
            else -> error("Pattern must be string or object, got: $tree")
        }
    }
}
