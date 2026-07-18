// /system-popups JSON assembly. The popup entry's id/type/source/title/
// body/buttons keys and the button entry's id/label/role keys are the
// contract with the Rust SystemPopup / SystemPopupButton deserializers.
// The `dangerous` flag is asserted below — the key used to be emitted
// as `destructive`, which the Rust deserializer silently dropped.

package dev.smix.runner

import org.json.JSONArray
import org.junit.Assert.assertEquals
import org.junit.Test

class PopupWireTest {

    // MARK: - buttonId

    @Test
    fun buttonIdPrefersShortIdOverLabelSlug() {
        assertEquals("confirm-btn", PopupWire.buttonId("confirm-btn", "Confirm"))
        assertEquals("confirm", PopupWire.buttonId("", "Confirm"))
    }

    // MARK: - buttonRole

    @Test
    fun buttonRoleCancelById() {
        assertEquals("cancel", PopupWire.buttonRole("cancel-btn", "Nope"))
        assertEquals("cancel", PopupWire.buttonRole("Cancel", "Cancel"))
    }

    @Test
    fun buttonRoleDestructiveByIdOrDeleteLabel() {
        assertEquals("destructive", PopupWire.buttonRole("destructive-btn", "Remove"))
        assertEquals("destructive", PopupWire.buttonRole("ok-btn", "Delete"))
        assertEquals("destructive", PopupWire.buttonRole("ok-btn", "delete"))
    }

    @Test
    fun buttonRoleDefaultOtherwise() {
        assertEquals("default", PopupWire.buttonRole("confirm-btn", "Confirm"))
    }

    // MARK: - buttonEntry

    @Test
    fun buttonEntryCarriesIdLabelRoleAndDangerous() {
        val obj = PopupWire.buttonEntry("confirm-btn", "Confirm", "default")
        assertEquals("confirm-btn", obj.getString("id"))
        assertEquals("Confirm", obj.getString("label"))
        assertEquals("default", obj.getString("role"))
        assertEquals(false, obj.getBoolean("dangerous"))
        assertEquals(
            true,
            PopupWire.buttonEntry("rm", "Remove", "destructive").getBoolean("dangerous"),
        )
    }

    // MARK: - popupEntry

    @Test
    fun popupEntryShape() {
        val buttons = JSONArray().put(PopupWire.buttonEntry("ok", "OK", "default"))
        val obj = PopupWire.popupEntry("android-popup-0", "com.example.app", "Title", "Body", buttons)
        assertEquals("android-popup-0", obj.getString("id"))
        assertEquals("alert", obj.getString("type"))
        assertEquals("com.example.app", obj.getString("source"))
        assertEquals("Title", obj.getString("title"))
        assertEquals("Body", obj.getString("body"))
        assertEquals(1, obj.getJSONArray("buttons").length())
    }

    @Test
    fun popupEntryNullTitleBodyBecomeEmptyStrings() {
        val obj = PopupWire.popupEntry("android-popup-0", "com.example.app", null, null, JSONArray())
        assertEquals("", obj.getString("title"))
        assertEquals("", obj.getString("body"))
    }
}
