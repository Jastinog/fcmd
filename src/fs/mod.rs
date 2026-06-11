//! Filesystem operations.
//!
//! - [`ops`]: copy / move / delete / rename / mkdir / touch, the yank-register
//!   and the undo stack, plus conflict-resolution and progress reporting.
//! - [`du`]: recursive directory-size calculation.
//! - [`perms`]: `chmod` / `chown`.

pub mod du;
pub mod ops;
pub mod perms;
