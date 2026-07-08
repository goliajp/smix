#![doc = include_str!("../README.md")]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![doc(html_root_url = "https://docs.smix.dev/smix-core")]

//! Internal layout: this crate is intentionally tiny. The sense /
//! decide / act primitives live in dedicated stones (see the table in
//! README); `smix-core` itself just exposes the package version as a
//! smoke-test compile target so `cargo install smix-core` resolves.

/// Compile-time crate version. Validates the build link from a `cargo
/// test` smoke perspective.
#[doc(hidden)]
pub const __CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");
