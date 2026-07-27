//! Shared utility code for the workspace.
//!
//! At the moment this crate primarily provides buffer pooling helpers used by
//! the decoders to batch events without allocating a new `Vec` for every batch.

/// Recyclable buffer types used for decoded event batches.
pub mod buffer;
