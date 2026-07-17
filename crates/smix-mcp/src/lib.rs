//! The parts of the MCP server worth testing on their own.
//!
//! The binary wires these into an `rmcp` router and talks stdio; what lives
//! here is the translation between what an agent writes and what the SDK
//! takes.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod selector_params;

pub use selector_params::SelectorParams;
