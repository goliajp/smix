//! Where an element is: its box and the screen's, waited for the way the
//! other element verbs wait.

use smix_sdk::{ExpectationFailure, FailureCode, FailureInit, Selector};

use super::{Adapter, IMPLICIT_WAIT, RunError};
use crate::AppLike;

impl<A: AppLike + ?Sized> Adapter<'_, A> {
    /// What `cropOn` crops to: the element's box and the root of the
    /// same tree, both in the tree's units, so the scale to the
    /// screenshot comes from two readings of one screen.
    pub(super) async fn crop_target(
        &self,
        selector: &Selector,
    ) -> Result<smix_sdk::CropTo, RunError> {
        let (root, element) = self
            .wait_for_bounds(selector, "cropOn: no element to crop to")
            .await?;
        Ok(smix_sdk::CropTo { element, root })
    }

    /// The tree root's bounds and the matched element's, waiting for it
    /// the way the other element verbs do. Not found by the deadline →
    /// `ElementNotFound` opening with `what`, carrying the screen it
    /// last read.
    pub(super) async fn wait_for_bounds(
        &self,
        selector: &Selector,
        what: &str,
    ) -> Result<(smix_sdk::Rect, smix_sdk::Rect), RunError> {
        let budget = smix_driver::poll::Budget::new(IMPLICIT_WAIT);
        loop {
            let look = budget.look();
            let tree = self.app.tree().await?;
            if let Some(node) = smix_selector_resolver::resolve_selector(&tree, selector) {
                return Ok((tree.bounds, node.bounds));
            }
            if budget.spent_by(look) {
                return Err(RunError::Sdk(ExpectationFailure::new(
                    FailureInit {
                        code: Some(FailureCode::ElementNotFound),
                        message: format!("{what}: {}", smix_selector::describe_selector(selector)),
                        selector: Some(selector.clone()),
                        ..Default::default()
                    }
                    .with_screen(smix_sdk::screen_facts(&tree, 10)),
                )));
            }
            tokio::time::sleep(budget.pause()).await;
        }
    }
}
