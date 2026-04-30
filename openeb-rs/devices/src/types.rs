use macros::derive_value;
use openeb_core::hal::decoders::evt3::Evt3Decoder;
use std::fmt::Display;
use thiserror::Error;

// --- Supporting Types ---

#[derive(Error, Debug)]
pub enum DeviceFileError {
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Unsupported format: {0}")]
    Format(String),
    #[error("Could not find geometry in header")]
    UnknownGeometry(),
    #[error("Could not parse geometry as an integer: {0}")]
    GeometryParsing(#[from] std::num::ParseIntError),
    #[error("End of file reached")]
    EOF(),
}

#[derive_value]
pub enum FileFormat {
    EVT2,
    EVT3,
    DAT,
    HDF5,
    UNKNOWN,
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
