//! Decoder implementations and decoder-related traits.
//!
//! The workspace currently treats decoders as the part of the HAL that turns raw
//! stream buffers into typed event batches. The `evt3` decoder is the most
//! complete implementation today; the other modules are present to support future
//! formats and protocol variants.

pub mod dat;
pub mod evt2;
pub mod evt3;
pub mod raw_fmt_decoder;
