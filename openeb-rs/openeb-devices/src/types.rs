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
    #[error(
        "Failed to acquire write lock. If you get this error, report a pull request because we failed internally."
    )]
    WriteLockError,
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

#[derive(Error, Debug)]
pub enum IteratorError {}

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

pub enum FormatDecoder {
    Evt3(Evt3Decoder),
    Unknown,
}

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
    pub fn new() -> Self {
        RawEventBuffer { _data: [0u8; N] }
    }

    pub fn resize<const M: usize>(self) -> RawEventBuffer<M> {
        let mut new_buffer = RawEventBuffer::<M>::new();
        let copy_len = std::cmp::min(N, M);
        new_buffer._data[..copy_len].copy_from_slice(&self._data[..copy_len]);
        new_buffer
    }
}

#[derive(Clone, Debug)]
pub struct IndexMarker {
    pub byte_offset: u64,
    pub timestamp: usize,
    pub state: DecoderTimingState,
}

#[derive(Clone, Debug, Default)]
pub struct FileIndex {
    pub markers: Vec<IndexMarker>,
}

impl FileIndex {
    /// Performs a binary search to find the closest index marker occurring *before or at* the target timestamp.
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
                    Some(&self.markers[0])
                } else {
                    Some(&self.markers[idx - 1])
                }
            }
        }
    }
}
