// /tree wire assembly: android class → smix Role mapping, resource-id
// short strip, A11yNode JSON shape (camelCase field names are the
// contract with smix-screen's A11yNode deserializer), virtual window
// root.

package dev.smix.runner

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TreeWireTest {

    // MARK: - deriveRole

    @Test
    fun deriveRoleMapsWidgetClasses() {
        assertEquals("button", TreeWire.deriveRole("android.widget.Button"))
        assertEquals("button", TreeWire.deriveRole("android.widget.ImageButton"))
        assertEquals("button", TreeWire.deriveRole("androidx.appcompat.widget.AppCompatButton"))
        assertEquals("textField", TreeWire.deriveRole("android.widget.EditText"))
        assertEquals("textField", TreeWire.deriveRole("androidx.appcompat.widget.AppCompatEditText"))
        assertEquals("image", TreeWire.deriveRole("android.widget.ImageView"))
        assertEquals("switch", TreeWire.deriveRole("android.widget.Switch"))
        assertEquals("switch", TreeWire.deriveRole("androidx.appcompat.widget.SwitchCompat"))
        assertEquals("checkBox", TreeWire.deriveRole("android.widget.CheckBox"))
        assertEquals("radio", TreeWire.deriveRole("android.widget.RadioButton"))
        assertEquals("switch", TreeWire.deriveRole("android.widget.ToggleButton"))
        assertEquals("staticText", TreeWire.deriveRole("android.widget.TextView"))
        assertEquals("staticText", TreeWire.deriveRole("androidx.appcompat.widget.AppCompatTextView"))
        assertEquals("scrollView", TreeWire.deriveRole("androidx.recyclerview.widget.RecyclerView"))
        assertEquals("scrollView", TreeWire.deriveRole("android.widget.ListView"))
        assertEquals("tabBar", TreeWire.deriveRole("com.google.android.material.tabs.TabLayout"))
        assertEquals("tabBar", TreeWire.deriveRole("android.widget.TabHost"))
        assertEquals("navigationBar", TreeWire.deriveRole("androidx.appcompat.widget.Toolbar"))
        assertEquals("webView", TreeWire.deriveRole("android.webkit.WebView"))
    }

    @Test
    fun deriveRoleUnknownClassReturnsNull() {
        assertNull(TreeWire.deriveRole("android.view.View"))
        assertNull(TreeWire.deriveRole("android.widget.FrameLayout"))
        assertNull(TreeWire.deriveRole(""))
    }

    // MARK: - shortResourceId

    @Test
    fun shortResourceIdStripsPackagePrefix() {
        assertEquals("submit-btn", TreeWire.shortResourceId("com.example.app:id/submit-btn"))
        assertEquals("submit-btn", TreeWire.shortResourceId("submit-btn"))
    }

    // MARK: - nodeJson

    @Test
    fun nodeJsonFullShape() {
        val obj = TreeWire.nodeJson(
            rawType = "android.widget.Button",
            identifier = "com.example.app:id/submit-btn",
            label = "Submit",
            text = "Submit now",
            x = 10,
            y = 20,
            w = 100,
            h = 40,
            enabled = true,
            selected = false,
            hasFocus = true,
            visible = true,
            children = JSONArray(),
        )
        assertEquals("android.widget.Button", obj.getString("rawType"))
        assertEquals("button", obj.getString("role"))
        assertEquals("submit-btn", obj.getString("identifier"))
        assertEquals("Submit", obj.getString("label"))
        assertEquals("Submit now", obj.getString("text"))
        val bounds = obj.getJSONObject("bounds")
        assertEquals(10, bounds.getInt("x"))
        assertEquals(20, bounds.getInt("y"))
        assertEquals(100, bounds.getInt("w"))
        assertEquals(40, bounds.getInt("h"))
        assertTrue(obj.getBoolean("enabled"))
        assertFalse(obj.getBoolean("selected"))
        assertTrue(obj.getBoolean("hasFocus"))
        assertTrue(obj.getBoolean("visible"))
        assertEquals(0, obj.getJSONArray("children").length())
    }

    @Test
    fun nodeJsonOmitsEmptyOptionalFieldsAndUnknownRole() {
        val obj = TreeWire.nodeJson(
            rawType = "android.view.View",
            identifier = null,
            label = "",
            text = null,
            x = 0,
            y = 0,
            w = 0,
            h = 0,
            enabled = true,
            selected = false,
            hasFocus = false,
            visible = false,
            children = JSONArray(),
        )
        assertFalse(obj.has("role"))
        assertFalse(obj.has("identifier"))
        assertFalse(obj.has("label"))
        assertFalse(obj.has("text"))
    }

    @Test
    fun nodeJsonClampsNegativeExtentsToZero() {
        val obj = TreeWire.nodeJson(
            rawType = "android.view.View",
            identifier = null,
            label = null,
            text = null,
            x = 5,
            y = 6,
            w = -3,
            h = -1,
            enabled = true,
            selected = false,
            hasFocus = false,
            visible = false,
            children = JSONArray(),
        )
        val bounds = obj.getJSONObject("bounds")
        assertEquals(0, bounds.getInt("w"))
        assertEquals(0, bounds.getInt("h"))
    }

    @Test
    fun nodeJsonNestsChildren() {
        val child = TreeWire.nodeJson(
            rawType = "android.widget.TextView",
            identifier = null,
            label = null,
            text = "hi",
            x = 0, y = 0, w = 10, h = 10,
            enabled = true, selected = false, hasFocus = false, visible = true,
            children = JSONArray(),
        )
        val obj = TreeWire.nodeJson(
            rawType = "android.widget.FrameLayout",
            identifier = null,
            label = null,
            text = null,
            x = 0, y = 0, w = 100, h = 100,
            enabled = true, selected = false, hasFocus = false, visible = true,
            children = JSONArray().put(child),
        )
        val children = obj.getJSONArray("children")
        assertEquals(1, children.length())
        assertEquals("hi", children.getJSONObject(0).getString("text"))
    }

    // MARK: - windowRootJson

    @Test
    fun windowRootJsonShape() {
        val root = TreeWire.windowRootJson(1080, 2400, JSONArray().put(JSONObject()))
        assertEquals("android.view.WindowRoot", root.getString("rawType"))
        val bounds = root.getJSONObject("bounds")
        assertEquals(0, bounds.getInt("x"))
        assertEquals(0, bounds.getInt("y"))
        assertEquals(1080, bounds.getInt("w"))
        assertEquals(2400, bounds.getInt("h"))
        assertTrue(root.getBoolean("enabled"))
        assertFalse(root.getBoolean("selected"))
        assertFalse(root.getBoolean("hasFocus"))
        assertTrue(root.getBoolean("visible"))
        assertEquals(1, root.getJSONArray("children").length())
    }
}
