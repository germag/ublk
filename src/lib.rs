// SPDX-License-Identifier: MIT

//! Rust friendly library for Userspace block driver (ublk)
//!
//! This library allows the implementation of generic userspace
//! block devices.
//!
//! ublk aims to be minimal and misuse-resistant.
#[deny(unsafe_op_in_unsafe_fn)]
//#[warn(missing_crate_level_docs, missing_docs)]
pub mod control;
