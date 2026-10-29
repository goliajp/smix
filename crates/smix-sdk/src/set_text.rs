//! Entering text without the keyboard.

use smix_error::ExpectationFailure;
use smix_selector::Selector;

use crate::{App, focused_fill_refusal, now_ms};

impl App {
    /// [`Self::fill`] without the keyboard: the field is given its new
    /// content as a whole (`ACTION_SET_TEXT` on Android), so an input
    /// method that composes typed keys into another script cannot change
    /// it. Replaces a named field and appends to the focused one, as
    /// `fill` does. iOS refuses: XCUITest enters text only by typing.
    pub async fn set_text(&self, selector: &Selector, text: &str) -> Result<(), ExpectationFailure> {
        self.ledger
            .record_fill(now_ms(), Some(format!("{selector:?}")));
        let names_a_field = !matches!(selector, Selector::Focused { .. });
        let out = self
            .driving()?
            .set_text(selector, text, None, names_a_field)
            .await;
        match (out, names_a_field) {
            (Err(e), false) => Err(focused_fill_refusal(e)),
            (other, _) => other,
        }
    }
}
