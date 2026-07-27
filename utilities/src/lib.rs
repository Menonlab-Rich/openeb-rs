//! Shared utility code for the workspace.
//!
//! At the moment this crate primarily provides buffer pooling helpers used by
//! the decoders to batch events without allocating a new `Vec` for every batch.

pub mod buffer;
