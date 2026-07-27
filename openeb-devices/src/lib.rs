//! File-backed device support for `openeb-rs`.
//!
//! This crate focuses on opening raw event files, parsing their metadata header,
//! wiring up the appropriate facilities, and exposing decoded event streams to
//! consumers.
//!
//! The main entry points are:
//!
//! - `RawFileReader`, which opens a file and drives decoding
//! - `EventWindowIterator`, which consumes decoded events in batches or time windows
//! - `RREventStreamDecoder`, which wraps the format-specific raw decoders
//!
//! The public API is intentionally built on top of the `openeb-core` facility
//! model so that file-backed devices look like other devices in the workspace.

pub mod device_macros;
pub mod header;
mod raw;
pub mod types;

pub use raw::{EventWindowIterator, RREventStreamDecoder, RawFileReader};
