//! The parts of the MCP server worth testing on their own.
//!
//! The binary wires these into an `rmcp` router and talks stdio; what lives
//! here is the translation between what an agent writes and what the SDK
//! takes.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod selector_params;
pub mod session;

pub use selector_params::{SelectorParams, chain_of, ocr_locales_of, ocr_text_of, point_of};
pub use session::{Bound, SessionState, UNBOUND_HINT, session_state_report};
