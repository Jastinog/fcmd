//! Core data model: what the panels display.
//!
//! - [`panel`]: the file listing — [`Panel`], [`FileEntry`], [`DirCache`], sorting
//!   and async directory loading.
//! - [`tree`]: the tree-view line builder.

pub mod panel;
pub mod tree;
