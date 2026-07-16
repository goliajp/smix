// Role enum mirrors Swift SmixSDK.Role + Rust smix-screen Role.
//
// Wire form: lowercase camelCase ("button" / "textField" / etc.) per
// Rust `#[serde(rename_all = "camelCase")]`.

package dev.smix.sdk

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
enum class A11yRole {
    @SerialName("button") BUTTON,
    @SerialName("link") LINK,
    @SerialName("textField") TEXT_FIELD,
    @SerialName("secureTextField") SECURE_TEXT_FIELD,
    @SerialName("searchField") SEARCH_FIELD,
    @SerialName("switch") SWITCH,
    @SerialName("toggle") TOGGLE,
    @SerialName("checkBox") CHECK_BOX,
    @SerialName("radio") RADIO,
    @SerialName("image") IMAGE,
    @SerialName("staticText") STATIC_TEXT,
    @SerialName("tab") TAB,
    @SerialName("tabBar") TAB_BAR,
    @SerialName("navigationBar") NAVIGATION_BAR,
    @SerialName("cell") CELL,
    @SerialName("alert") ALERT,
    @SerialName("dialog") DIALOG,
    @SerialName("slider") SLIDER,
    @SerialName("progressBar") PROGRESS_BAR,
    @SerialName("picker") PICKER,
    @SerialName("menu") MENU,
    @SerialName("menuItem") MENU_ITEM,
    @SerialName("scrollView") SCROLL_VIEW,
    @SerialName("segmentedControl") SEGMENTED_CONTROL,
    @SerialName("table") TABLE,
    @SerialName("collectionView") COLLECTION_VIEW,
    @SerialName("webView") WEB_VIEW,
    @SerialName("keyboard") KEYBOARD,
}
