//! Shared types used by the raw file reader and iterator layers.
//!
//! These types cover errors, file format handling, raw buffer storage, and
//! indexing support for timestamp-based seeking.

use crossbeam::channel::TryRecvError;
use macros::derive_value;
pub use openeb_core::hal::types::{EventCD, EventExtTrigger};
use openeb_core::hal::{
    decoders::evt3::{DecoderTimingState, Evt3Decoder},
    facilities::{self, FacilityError},
};
use std::fmt::Display;
use thiserror::Error;

// --- Supporting Types ---

/// Errors that can occur while opening or reading a raw event file.
#[derive(Error, Debug)]
pub enum DeviceFileError {
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Try Recv Error: {0}")]
    TryRecv(#[from] TryRecvError),
    #[error("Unsupported format: {0}")]
    Format(String),
    #[error("Could not find geometry in header")]
    UnknownGeometry(),
    #[error("Could not parse geometry as an integer: {0}")]
    GeometryParsing(#[from] std::num::ParseIntError),
    #[error("End of file reached")]
    EOF(),
    #[error("Unsupported facility: {0}")]
    UnsupportedFacility(String),
    #[error("Lock was poisoned")]
    LockError,
    #[error("Facility Error: {0}")]
    FacilityTypeMismatch(#[from] facilities::FacilityTypeMismatch),
    #[error("Unregistred Facility: {0}")]
    UnregisteredFacility(String),
    #[error(transparent)]
    FacilityError(#[from] FacilityError),
    #[error("Attempted to execute unsupported behavior: {0}")]
    UnsupportedBehavior(String),
    #[error("Method called on unitialized device!")]
    NotInitialized,
}

/// Placeholder error type for iterator-specific failures.
///
/// TODO: define concrete iterator failure modes or remove this type if it is no
/// longer needed by the public API.
#[derive(Error, Debug)]
pub enum IteratorError {}

/// File format identifier parsed from the raw header.
///
/// The default is `EVT3` because that is the currently supported decoder path.
#[derive_value]
pub enum FileFormat {
    EVT2,
    EVT3,
    DAT,
    HDF5,
    UNKNOWN,
}

impl Default for FileFormat {
    fn default() -> Self {
        FileFormat::EVT3
    }
}
impl Display for FileFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileFormat::EVT2 => write!(f, "evt 2.0"),
            FileFormat::EVT3 => write!(f, "evt 3.0"),
            FileFormat::DAT => write!(f, "dat"),
            FileFormat::HDF5 => write!(f, "hdf5"),
            _ => write!(f, "UNKNOWN"),
        }
    }
}

/// Decoder selection used by higher-level code.
///
/// TODO: this currently only models the EVT3 decoder path. Either expand this
/// enum or replace it with a more direct abstraction if additional formats are
/// added.
pub enum FormatDecoder {
    Evt3(Evt3Decoder),
    Unknown,
}

/// Fixed-size byte buffer used to read raw stream chunks.
#[derive(Debug, Clone, Copy)]
pub struct RawEventBuffer<const N: usize> {
    _data: [u8; N],
}

impl<const N: usize> AsRef<[u8]> for RawEventBuffer<N> {
    fn as_ref(&self) -> &[u8] {
        &self._data
    }
}

impl<const N: usize> AsMut<[u8]> for RawEventBuffer<N> {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self._data
    }
}

impl<const N: usize> std::ops::Deref for RawEventBuffer<N> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<const N: usize> std::ops::DerefMut for RawEventBuffer<N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut()
    }
}

impl<const N: usize> RawEventBuffer<N> {
    /// Creates a zero-initialized buffer.
    pub fn new() -> Self {
        RawEventBuffer { _data: [0u8; N] }
    }

    /// Resizes the buffer, preserving as much data as will fit.
    pub fn resize<const M: usize>(self) -> RawEventBuffer<M> {
        let mut new_buffer = RawEventBuffer::<M>::new();
        let copy_len = std::cmp::min(N, M);
        new_buffer._data[..copy_len].copy_from_slice(&self._data[..copy_len]);
        new_buffer
    }
}

/// Marker describing a point in the file index.
#[derive(Clone, Debug)]
pub struct IndexMarker {
    pub byte_offset: u64,
    pub timestamp: usize,
    pub state: DecoderTimingState,
}

/// Timestamp index for raw files.
///
/// The index stores sampled byte offsets and decoder timing state so the reader
/// can jump near a requested timestamp and restore decoder state before
/// continuing.
#[derive(Clone, Debug, Default)]
pub struct FileIndex {
    pub t_min: usize,
    pub t_max: usize,
    pub markers: Vec<IndexMarker>,
}

impl FileIndex {
    /// Returns the closest marker occurring before or at `target_ts`.
    pub fn find_closest_marker(&self, target_ts: usize) -> Option<&IndexMarker> {
        if self.markers.is_empty() {
            return None;
        }

        match self
            .markers
            .binary_search_by_key(&target_ts, |m| m.timestamp)
        {
            Ok(idx) => Some(&self.markers[idx]),
            Err(idx) => {
                // If not found, `idx` is the piece where it *would* be inserted.
                // We want the marker right before it.
                if idx == 0 {
                    None
                } else {
                    Some(&self.markers[idx - 1])
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_event_buffer_is_zeroed_and_resizes_in_both_directions() {
        let mut source = RawEventBuffer::<4>::new();
        source.copy_from_slice(&[1, 2, 3, 4]);

        let larger = source.resize::<6>();
        assert_eq!(larger.as_ref(), &[1, 2, 3, 4, 0, 0]);

        let smaller = larger.resize::<2>();
        assert_eq!(smaller.as_ref(), &[1, 2]);
        assert_eq!(RawEventBuffer::<3>::new().as_ref(), &[0, 0, 0]);
    }

    #[test]
    fn index_lookup_returns_only_markers_at_or_before_target() {
        let index = FileIndex {
            markers: vec![
                IndexMarker {
                    byte_offset: 0,
                    timestamp: 10,
                    state: DecoderTimingState::default(),
                },
                IndexMarker {
                    byte_offset: 20,
                    timestamp: 20,
                    state: DecoderTimingState::default(),
                },
            ],
            ..FileIndex::default()
        };

        assert!(index.find_closest_marker(9).is_none());
        assert_eq!(index.find_closest_marker(10).unwrap().byte_offset, 0);
        assert_eq!(index.find_closest_marker(19).unwrap().byte_offset, 0);
        assert_eq!(index.find_closest_marker(99).unwrap().byte_offset, 20);
    }
}
