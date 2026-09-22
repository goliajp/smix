package dev.smix.runner

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/**
 * A node the probe reported gets the same role name the accessibility path
 * would have given it.
 *
 * The two readers answer the same question and a flow cannot tell which
 * one is behind it, so `role:button` matching through one and not the
 * other is the failure this whole version is about — one field over from
 * the interop hole.
 */
class ProbeRolesTest {
    @Test
    fun `a hosted view is named from its class, by the table the other reader uses`() {
        assertEquals("button", ProbeRoles.roleOf(null, "android.widget.ImageButton"))
        assertEquals("staticText", ProbeRoles.roleOf(null, "android.widget.TextView"))
        assertEquals("textField", ProbeRoles.roleOf(null, "android.widget.EditText"))
    }

    @Test
    fun `compose spells its roles differently and is translated, not lowercased`() {
        // `Checkbox`.lowercase() is "checkbox"; the wire spells it "checkBox".
        assertEquals("checkBox", ProbeRoles.roleOf("Checkbox", null))
        assertEquals("radio", ProbeRoles.roleOf("RadioButton", null))
        assertEquals("picker", ProbeRoles.roleOf("DropdownList", null))
    }

    @Test
    fun `what compose says about a node beats what its class would say`() {
        assertEquals("tab", ProbeRoles.roleOf("Tab", "android.widget.TextView"))
    }

    @Test
    fun `a node neither fact names keeps no role at all`() {
        assertEquals(null, ProbeRoles.roleOf(null, "android.view.View"))
        assertEquals(null, ProbeRoles.roleOf(null, null))
    }

    @Test
    fun `filling walks the whole tree, children included`() {
        val filled = ProbeRoles.fill(
            """{"screen":[1080,2340],"roots":[
                 {"id":1,"testTag":"root","children":[
                   {"id":2,"resourceId":"btn_player_fullscreen",
                    "className":"android.widget.ImageButton","children":[]}]}]}""",
        )
        val root = JSONObject(filled).getJSONArray("roots").getJSONObject(0)
        val child = root.getJSONArray("children").getJSONObject(0)
        assertEquals("button", child.getString("role"))
    }

    @Test
    fun `a role this build cannot name is removed rather than passed on in another spelling`() {
        val filled = ProbeRoles.fill(
            """[{"id":1,"testTag":"a","role":"Holodeck","children":[]}]""",
        )
        assertFalse(
            "an unknown Compose role reached the host in Compose's spelling: $filled",
            filled.contains("Holodeck"),
        )
    }

    @Test
    fun `a payload that is not a tree comes back as it went in`() {
        // The probe answers "[]" while the app is starting, and its own
        // error shapes are not trees either. Losing those would turn "the
        // probe said why not" into "nothing came back".
        assertEquals("[]", ProbeRoles.fill("[]"))
        assertEquals("not json", ProbeRoles.fill("not json"))
    }
}
