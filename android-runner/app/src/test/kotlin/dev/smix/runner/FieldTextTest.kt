// What a field holds, read one way: an empty field that is showing its
// hint holds nothing. Since API 26 an empty EditText reports its hint as
// the node's text, with isShowingHintText set, and every reader that
// took `node.text` as content counted the hint as typed characters.

package dev.smix.runner

import org.json.JSONArray
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class FieldTextTest {

    @Test
    fun aFieldShowingItsHintHoldsNothing() {
        assertEquals("", FieldText.held("Search…", true))
    }

    @Test
    fun aFieldNotShowingItsHintHoldsItsText() {
        assertEquals("abc", FieldText.held("abc", false))
    }

    @Test
    fun aFieldWithNoTextHoldsNothing() {
        assertEquals("", FieldText.held(null, false))
    }

    @Test
    fun theHintTravelsAsPlaceholderValueAndNotAsText() {
        val obj = TreeWire.nodeJson(
            rawType = "android.widget.EditText",
            identifier = null,
            label = null,
            text = FieldText.held("type here", true),
            placeholder = "type here",
            x = 0, y = 0, w = 10, h = 10,
            enabled = true, selected = false, hasFocus = false, visible = true,
            children = JSONArray(),
        )
        assertEquals("type here", obj.getString("placeholderValue"))
        assertFalse(obj.has("text"))
    }
}
