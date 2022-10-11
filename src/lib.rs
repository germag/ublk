// SPDX-License-Identifier: MIT

//! Rust friendly library for Userspace block driver (ublk)
//!
//! This library allows the implementation of generic userspace
//! block devices.
//!
//! ublk aims to be minimal and misuse-resistant.

#[deny(unsafe_op_in_unsafe_fn)]
#[warn(rustdoc::missing_crate_level_docs, missing_docs)]
#[warn(
    clippy::missing_errors_doc,
    clippy::missing_safety_doc,
    clippy::missing_panics_doc,
    clippy::doc_markdown
)]
/// It contains the control paths
pub mod control;

/// Library errors
pub mod error;
pub use error::{Error, Result};
