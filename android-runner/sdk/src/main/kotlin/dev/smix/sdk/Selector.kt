// v7.4 c4 — Selector full 7-case + Modifiers flatten + Pattern + fluent.
//
// Mirror Swift v7.2 c3 + Rust smix-selector `#[serde(untagged)] enum
// Selector` with full `Modifiers` flatten on Text/Id/Label/Role/
// LocalizedText bases. Focused has no modifiers. Anchor uses
// AnchorBox + IndexModifiers.
//
// Fluent chaining:
//   Selector.Id("btn-login")
//     .below(Selector.Text(Pattern.Literal("Sign In")))
//     .nth(0)
// Each method returns a new Selector with Modifiers copied + mutated
// (immutable copy semantics).

package dev.smix.sdk

import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.MapSerializer
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.descriptors.buildClassSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonEncoder
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/**
 * A11y selector — identifies a node in the tree. Wire JSON shape
 * matches Rust smix-selector `#[serde(untagged)]`: discriminator is
 * which key is present. Modifier fields are flattened into the same
 * JSON body via [SelectorSerializer].
 */
@Serializable(with = SelectorSerializer::class)
sealed interface Selector {
    data class Id(val id: String, val modifiers: Modifiers = Modifiers.EMPTY) : Selector
    data class Text(val text: Pattern, val modifiers: Modifiers = Modifiers.EMPTY) : Selector
    data class Label(val label: String, val modifiers: Modifiers = Modifiers.EMPTY) : Selector
    data class Role(
        val role: String,
        val name: Pattern? = null,
        val modifiers: Modifiers = Modifiers.EMPTY,
    ) : Selector

    object Focused : Selector
    data class Anchor(
        val box: AnchorBox,
        val index: IndexModifiers = IndexModifiers.EMPTY,
    ) : Selector

    data class LocalizedText(
        val map: Map<String, String>,
        val modifiers: Modifiers = Modifiers.EMPTY,
    ) : Selector

    // ---- Fluent chaining methods --------------------------------------

    fun below(anchor: Selector): Selector = withModifiers { it.copy(below = anchor) }
    fun above(anchor: Selector): Selector = withModifiers { it.copy(above = anchor) }
    fun leftOf(anchor: Selector): Selector = withModifiers { it.copy(leftOf = anchor) }
    fun rightOf(anchor: Selector): Selector = withModifiers { it.copy(rightOf = anchor) }
    fun near(anchor: Selector): Selector = withModifiers { it.copy(near = anchor) }
    fun inside(anchor: Selector): Selector = withModifiers { it.copy(inside = anchor) }
    fun ancestor(anchor: Selector): Selector = withModifiers { it.copy(ancestor = anchor) }
    fun nth(index: Int): Selector = withModifiers { it.copy(nth = index) }
    fun first(): Selector = withModifiers { it.copy(first = true) }
    fun last(): Selector = withModifiers { it.copy(last = true) }

    private fun withModifiers(mutate: (Modifiers) -> Modifiers): Selector = when (this) {
        is Id -> Id(id, mutate(modifiers))
        is Text -> Text(text, mutate(modifiers))
        is Label -> Label(label, mutate(modifiers))
        is Role -> Role(role, name, mutate(modifiers))
        is LocalizedText -> LocalizedText(map, mutate(modifiers))
        is Focused -> this  // no modifiers — no-op
        is Anchor -> {
            // Anchor uses IndexModifiers — adapt only nth/first/last.
            val proxy = mutate(Modifiers.EMPTY)
            val newIdx = IndexModifiers(
                nth = proxy.nth ?: index.nth,
                first = proxy.first ?: index.first,
                last = proxy.last ?: index.last,
            )
            Anchor(box, newIdx)
        }
    }
}

/**
 * Custom KSerializer for [Selector] — emits Rust-compatible untagged
 * JSON with [Modifiers] flattened into the body. Discriminator on
 * decode = which key is present.
 */
object SelectorSerializer : KSerializer<Selector> {
    override val descriptor: SerialDescriptor =
        buildClassSerialDescriptor("dev.smix.sdk.Selector")

    // encodeDefaults = false so AnchorBox null fields skip rather than
    // emit `null` entries. Modifiers is encoded manually via addModifiers
    // (only non-null entries appear), so its defaults don't depend on
    // this setting.
    private val jsonFormat = Json { encodeDefaults = false; ignoreUnknownKeys = true }
    private val stringMapSerializer = MapSerializer(String.serializer(), String.serializer())

    override fun serialize(encoder: Encoder, value: Selector) {
        val jsonEncoder = encoder as? JsonEncoder
            ?: error("SelectorSerializer requires JSON encoder")
        val map = mutableMapOf<String, JsonElement>()

        when (value) {
            is Selector.Id -> {
                map["id"] = JsonPrimitive(value.id)
                addModifiers(map, value.modifiers)
            }
            is Selector.Text -> {
                map["text"] = jsonFormat.encodeToJsonElement(PatternSerializer, value.text)
                addModifiers(map, value.modifiers)
            }
            is Selector.Label -> {
                map["label"] = JsonPrimitive(value.label)
                addModifiers(map, value.modifiers)
            }
            is Selector.Role -> {
                map["role"] = JsonPrimitive(value.role)
                value.name?.let {
                    map["name"] = jsonFormat.encodeToJsonElement(PatternSerializer, it)
                }
                addModifiers(map, value.modifiers)
            }
            is Selector.Focused -> {
                map["focused"] = JsonPrimitive(true)
            }
            is Selector.Anchor -> {
                map["anchor"] = jsonFormat.encodeToJsonElement(AnchorBox.serializer(), value.box)
                addIndex(map, value.index)
            }
            is Selector.LocalizedText -> {
                map["localizedText"] =
                    jsonFormat.encodeToJsonElement(stringMapSerializer, value.map)
                addModifiers(map, value.modifiers)
            }
        }

        jsonEncoder.encodeJsonElement(JsonObject(map))
    }

    override fun deserialize(decoder: Decoder): Selector {
        val jsonDecoder = decoder as? JsonDecoder
            ?: error("SelectorSerializer requires JSON decoder")
        val obj = (jsonDecoder.decodeJsonElement() as? JsonObject)
            ?: error("Selector must be a JSON object")

        // Focused: explicit boolean field, highest-priority discriminator.
        obj["focused"]?.let {
            if (it.jsonPrimitive.booleanOrNull == true) return Selector.Focused
        }

        // Anchor: nested AnchorBox object discriminator.
        obj["anchor"]?.let { anchorJson ->
            val box = jsonFormat.decodeFromJsonElement(AnchorBox.serializer(), anchorJson)
            return Selector.Anchor(box, decodeIndex(obj))
        }

        // text / id / label / role / localizedText
        obj["id"]?.jsonPrimitive?.contentOrNull?.let { id ->
            return Selector.Id(id, decodeModifiers(obj))
        }
        obj["text"]?.let { textJson ->
            val pattern = jsonFormat.decodeFromJsonElement(PatternSerializer, textJson)
            return Selector.Text(pattern, decodeModifiers(obj))
        }
        obj["label"]?.jsonPrimitive?.contentOrNull?.let { lab ->
            return Selector.Label(lab, decodeModifiers(obj))
        }
        obj["role"]?.jsonPrimitive?.contentOrNull?.let { role ->
            val name = obj["name"]?.let {
                jsonFormat.decodeFromJsonElement(PatternSerializer, it)
            }
            return Selector.Role(role, name, decodeModifiers(obj))
        }
        obj["localizedText"]?.let { ltJson ->
            val map = jsonFormat.decodeFromJsonElement(stringMapSerializer, ltJson)
            return Selector.LocalizedText(map, decodeModifiers(obj))
        }
        error("Selector JSON missing recognized discriminator: $obj")
    }

    // ---- Modifier flatten helpers --------------------------------------

    private fun addModifiers(map: MutableMap<String, JsonElement>, m: Modifiers) {
        m.near?.let { map["near"] = jsonFormat.encodeToJsonElement(this, it) }
        m.below?.let { map["below"] = jsonFormat.encodeToJsonElement(this, it) }
        m.above?.let { map["above"] = jsonFormat.encodeToJsonElement(this, it) }
        m.leftOf?.let { map["leftOf"] = jsonFormat.encodeToJsonElement(this, it) }
        m.rightOf?.let { map["rightOf"] = jsonFormat.encodeToJsonElement(this, it) }
        m.inside?.let { map["inside"] = jsonFormat.encodeToJsonElement(this, it) }
        m.ancestor?.let { map["ancestor"] = jsonFormat.encodeToJsonElement(this, it) }
        m.nth?.let { map["nth"] = JsonPrimitive(it) }
        m.first?.let { map["first"] = JsonPrimitive(it) }
        m.last?.let { map["last"] = JsonPrimitive(it) }
    }

    private fun addIndex(map: MutableMap<String, JsonElement>, idx: IndexModifiers) {
        idx.nth?.let { map["nth"] = JsonPrimitive(it) }
        idx.first?.let { map["first"] = JsonPrimitive(it) }
        idx.last?.let { map["last"] = JsonPrimitive(it) }
    }

    private fun decodeModifiers(obj: JsonObject): Modifiers = Modifiers(
        near = obj["near"]?.let { jsonFormat.decodeFromJsonElement(this, it) },
        below = obj["below"]?.let { jsonFormat.decodeFromJsonElement(this, it) },
        above = obj["above"]?.let { jsonFormat.decodeFromJsonElement(this, it) },
        leftOf = obj["leftOf"]?.let { jsonFormat.decodeFromJsonElement(this, it) },
        rightOf = obj["rightOf"]?.let { jsonFormat.decodeFromJsonElement(this, it) },
        inside = obj["inside"]?.let { jsonFormat.decodeFromJsonElement(this, it) },
        ancestor = obj["ancestor"]?.let { jsonFormat.decodeFromJsonElement(this, it) },
        nth = obj["nth"]?.jsonPrimitive?.intOrNull,
        first = obj["first"]?.jsonPrimitive?.booleanOrNull,
        last = obj["last"]?.jsonPrimitive?.booleanOrNull,
    )

    private fun decodeIndex(obj: JsonObject): IndexModifiers = IndexModifiers(
        nth = obj["nth"]?.jsonPrimitive?.intOrNull,
        first = obj["first"]?.jsonPrimitive?.booleanOrNull,
        last = obj["last"]?.jsonPrimitive?.booleanOrNull,
    )
}
