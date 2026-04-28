use crossbeam::channel::Receiver;
use macros::{derive_value, pack_facility};
use openeb_core::hal::decoders::evt3::Evt3Decoder;
use openeb_core::hal::device::device::Device;
use openeb_core::hal::errors::{SharedError, StreamError};
use openeb_core::hal::facilities::{
    BaseDecoderFacility, ConnectionType, EventsStreamDecoderFacility, EventsStreamFacility,
    FacilityError, FacilityHandle, FacilityResult, FacilityType, GeometryFacility,
    HWIdentificationFacility, SensorInfo, SystemInfo,
};
use std::collections::HashMap;
use std::fmt::{Display, format};
use std::fs::File;
use std::io::{BufRead, Read};
use std::sync::Arc;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub format: FileFormat,
    pub width: u32,
    pub height: u32,
    pub metadata: HashMap<String, String>,
}

impl Header {
    pub fn parse<R: BufRead>(reader: &mut R) -> Result<Header, DeviceFileError> {
        let mut metadata = HashMap::new();

        loop {
            let buf = reader.fill_buf()?;
            if buf.is_empty() || buf[0] != b'%' {
                break;
            }

            let mut line = String::new();
            reader.read_line(&mut line)?;

            let line = line.trim();
            if let Some(rest) = line.strip_prefix('%') {
                let parts: Vec<&str> = rest.splitn(2, ':').collect();
                if parts.len() == 2 {
                    metadata.insert(parts[0].trim().to_string(), parts[1].trim().to_string());
                } else {
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    if parts.len() >= 2 {
                        metadata.insert(parts[0].to_string(), parts[1..].join(" "));
                    }
                }
            }
        }

        let raw_format_str = metadata
            .get("Data format")
            .or_else(|| metadata.get("format"))
            .map(|s| s.as_str())
            .unwrap_or("UNKNOWN");

        let parts = raw_format_str.splitn(3, ';');
        let mut format_str: Option<String> = None;
        let mut width: Option<String> = None;
        let mut height: Option<String> = None;
        for (i, p) in parts.enumerate() {
            match i {
                0 => format_str = Some(p.to_string()),
                1 => width = Some(p.to_string()),
                2 => height = Some(p.to_string()),
                _ => panic!("This should never happen"),
            }
        }

        let fmt_str = format_str.unwrap_or_else(|| raw_format_str.to_string());

        let format = match fmt_str.as_str() {
            "EVT2" => FileFormat::EVT2,
            "EVT3" => FileFormat::EVT3,
            "DAT" => FileFormat::DAT,
            "HDF5" => FileFormat::HDF5,
            _ => FileFormat::UNKNOWN,
        };

        if let Some(w) = width {
            metadata.insert("Geometry".to_string(), format!("{},{}", w, height.unwrap()));
        }

        let geometry_str = metadata
            .get("Geometry")
            .ok_or_else(|| metadata.get("geometry"))
            .or(Err(DeviceFileError::UnknownGeometry()))?;

        let coords = {
            if geometry_str.contains("=") {
                geometry_str
                    .split(',')
                    .try_fold(HashMap::<&str, &str>::new(), |mut acc, s| {
                        let parts: Vec<&str> = s.split("=").collect();
                        if parts.len() != 2 {
                            return Err(DeviceFileError::UnknownGeometry());
                        }
                        acc.insert(parts[0], parts[1]);
                        Ok(acc)
                    })
            } else {
                let mut coord_map = HashMap::<&str, &str>::new();
                let coord_values: Vec<&str> = geometry_str.split(',').collect();
                if coord_values.len() != 2 {
                    return Err(DeviceFileError::UnknownGeometry());
                }
                coord_map.insert("width", coord_values[0]);
                coord_map.insert("height", coord_values[1]);

                Ok(coord_map)
            }
        }?;
        if coords.len() != 2 {
            return Err(DeviceFileError::UnknownGeometry());
        }

        let width = coords
            .get("width")
            .ok_or(DeviceFileError::UnknownGeometry())?
            .parse::<u32>()?;
        let height = coords
            .get("height")
            .ok_or(DeviceFileError::UnknownGeometry())?
            .parse::<u32>()?;

        Ok(Header {
            format,
            width,
            height,
            metadata,
        })
    }
}

fn sensor_info_from_header(header: &Header) -> SensorInfo {
    let name = header
        .metadata
        .get("sensor_name")
        .map_or("UNKNOWN".to_string(), |v| v.to_string());
    let integrator = header
        .metadata
        .get("integrator_name")
        .map_or("UNKNOWN".to_string(), |v| v.to_string());
    let version = header
        .metadata
        .get("sensor_generation")
        .or_else(|| header.metadata.get("generation"))
        .map_or("x.x".to_string(), |v| v.to_string());

    SensorInfo {
        name,
        integrator,
        version,
    }
}

// --- Main Device ---

pub struct RawFileReader {
    header: Arc<Header>,
    facilities: HashMap<FacilityType, FacilityHandle>,
}

impl RawFileReader {
    fn new_from_path(path: &str) -> Result<Self, DeviceFileError> {
        let file = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(&file);
        let header = Header::parse(&mut reader)?;
        let header_arc = Arc::new(header);
        let mut device = RawFileReader {
            header: header_arc.clone(),
            facilities: HashMap::new(),
        };

        // Initialize and register Geometry Facility
        let geometry =
            RawReaderGeometry::new(device.header.width as i32, device.header.height as i32);
        device.register_facility(
            FacilityType::GeometryFacility,
            pack_facility!(ro GeometryFacility, geometry),
        );

        // Initialize and register HW Identification Facility
        let hw_ident = RawReaderHWIdentification {
            header: header_arc.clone(),
        };
        device.register_facility(
            FacilityType::HWIdentificationFacility,
            pack_facility!(ro HWIdentificationFacility, hw_ident),
        );

        let stream = RREventStream::new(file);
        device.register_facility(
            FacilityType::EventsStreamFacility,
            pack_facility!(mut EventsStreamFacility, stream),
        );

        let decoder = RREventStreamDecoder::new(&header_arc.clone(), true); // Assuming do_time_shift = true
        device.register_facility(
            FacilityType::EventsStreamDecoderFacility,
            pack_facility!(mut EventsStreamDecoderFacility, decoder),
        );

        Ok(device)
    }
}

// Corrected Device Trait Implementation
impl Device for RawFileReader {
    fn get_facility(
        &self,
        facility_type: FacilityType,
    ) -> Option<openeb_core::hal::facilities::FacilityHandle> {
        self.facilities.get(&facility_type).cloned()
    }

    fn get_facilities(&self) -> Vec<FacilityType> {
        self.facilities.keys().copied().collect()
    }

    fn register_facility(
        &mut self,
        facility_type: FacilityType,
        facility_handle: FacilityHandle,
    ) -> Option<FacilityHandle> {
        self.facilities.insert(facility_type, facility_handle)
    }
}

// --- Facility Implementations ---

struct RawReaderGeometry {
    width: i32,
    height: i32,
}

impl RawReaderGeometry {
    fn new(width: i32, height: i32) -> Self {
        RawReaderGeometry { width, height }
    }
}

impl GeometryFacility for RawReaderGeometry {
    fn get_width(&self) -> i32 {
        self.width
    }

    fn get_height(&self) -> i32 {
        self.height
    }
}

struct RawReaderHWIdentification {
    header: Arc<Header>,
}

impl HWIdentificationFacility for RawReaderHWIdentification {
    fn get_system_id(&self) -> FacilityResult<i64> {
        let id = self
            .header
            .metadata
            .get("system_ID")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        Ok(id)
    }

    fn get_serial(&self) -> FacilityResult<String> {
        let serial = self
            .header
            .metadata
            .get("serial_number")
            .cloned()
            .unwrap_or_else(|| "ffffffffffffffff".to_string());
        Ok(serial)
    }

    fn get_sensor_info(&self) -> FacilityResult<SensorInfo> {
        Ok(sensor_info_from_header(&self.header))
    }

    fn get_system_info(&self) -> FacilityResult<SystemInfo> {
        Ok(SystemInfo {
            serial_number: self.get_serial().unwrap_or_default(),
            firmware_version: self
                .header
                .metadata
                .get("firmaware_version")
                .or_else(|| self.header.metadata.get("firmware_version"))
                .cloned()
                .unwrap_or_else(|| "x.x".to_string()),
        })
    }

    fn get_connection_type(&self) -> FacilityResult<ConnectionType> {
        Ok(ConnectionType::Unknown)
    }

    fn get_available_data_encoding_formats(&self) -> FacilityResult<Vec<String>> {
        Ok(vec![self.header.format.to_string()])
    }

    fn get_current_data_encoding_format(&self) -> FacilityResult<String> {
        Ok(self.header.format.to_string())
    }
}

struct RREventStream {
    file: File,
    buffer: Vec<u8>,
    eof: bool,
}

impl RREventStream {
    const CHUNK_SIZE: usize = 131_072;
    fn new(file: File) -> Self {
        RREventStream {
            file,
            buffer: vec![0u8; RREventStream::CHUNK_SIZE],
            eof: false,
        }
    }
}

impl EventsStreamFacility for RREventStream {
    fn start(&mut self) -> FacilityResult<()> {
        Ok(()) // nothing to start when reading a file
    }
    fn stop(&mut self) -> FacilityResult<()> {
        Ok(()) // nothing to stop when reading a file
    }
    fn poll_buffer(&mut self) -> FacilityResult<(&[u8], usize)> {
        self.wait_next_buffer()
    }
    fn wait_next_buffer(&mut self) -> FacilityResult<(&[u8], usize)> {
        if self.eof {
            return Err(FacilityError::Stream(StreamError::EndOfFile));
        }

        self.buffer.resize(Self::CHUNK_SIZE, 0);

        match self.file.read(&mut self.buffer) {
            Ok(0) => {
                self.eof = true;
                Err(FacilityError::Stream(StreamError::EndOfFile))
            }
            Ok(bytes_read) => Ok((&self.buffer[..bytes_read], bytes_read)),
            Err(err) => {
                self.eof = true;
                Err(FacilityError::Stream(StreamError::IoError(err)))
            }
        }
    }
}

pub struct RREventStreamDecoder {
    decoder: Box<dyn EventsStreamDecoderFacility + Send + Sync>, // Added Send + Sync if multi-threading is required
    pub event_format: FileFormat,
}

impl RREventStreamDecoder {
    pub fn new(header: &Header, do_time_shift: bool) -> Self {
        // Box the specific decoder implementations to unify their type
        let decoder: Box<dyn EventsStreamDecoderFacility + Send + Sync> = match header.format {
            FileFormat::EVT2 => todo!(),
            FileFormat::EVT3 => Box::new(Evt3Decoder::new(
                header.width as u16,
                header.height as u16,
                do_time_shift,
            )),
            FileFormat::DAT => todo!(),
            FileFormat::HDF5 => todo!(),
            FileFormat::UNKNOWN => todo!(),
        };

        Self {
            event_format: header.format.clone(),
            decoder,
        }
    }
}

impl EventsStreamDecoderFacility for RREventStreamDecoder {
    fn decode(&mut self, raw_data: &[u8]) -> FacilityResult<()> {
        self.decoder.decode(raw_data)
    }

    fn get_last_timestamp(&self) -> usize {
        self.decoder.get_last_timestamp()
    }

    fn get_timestamp_shift(&self) -> Option<usize> {
        self.decoder.get_timestamp_shift()
    }

    fn is_time_shifting_enabled(&self) -> bool {
        self.decoder.is_time_shifting_enabled()
    }

    fn reset_last_timestamp(&mut self, timestamp: usize) {
        self.decoder.reset_last_timestamp(timestamp)
    }

    fn reset_timestamp_shift(&mut self, shift: usize) {
        self.decoder.reset_timestamp_shift(shift)
    }

    fn is_decoded_event_stream_indexable(&self) -> bool {
        self.decoder.is_decoded_event_stream_indexable()
    }
}

impl BaseDecoderFacility for RREventStreamDecoder {
    fn subscribe_to_protocol_violation(&mut self) -> Receiver<SharedError> {
        self.decoder.subscribe_to_protocol_violation()
    }
    fn get_raw_event_size_bytes(&self) -> FacilityResult<u8> {
        Ok(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*; // Adjust imports based on your file structure
    use openeb_core::hal::errors::StreamError;
    use openeb_core::hal::facilities::{
        EventsStreamDecoderFacilityHandle, EventsStreamFacilityHandle, FacilityError, FacilityType,
    };
    use std::path::PathBuf;

    #[test]
    fn test_read_and_decode_raw_evt3() -> Result<(), Box<dyn std::error::Error>> {
        // Point this to a valid .raw file in your test directory
        let mut file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        file_path.push("tests");
        file_path.push("sample.raw");

        // 1. Device Initialization
        let device = RawFileReader::new_from_path(file_path.into_os_string().to_str().expect("A cargo manifest dir must be specified."))
            .expect("Failed to initialize device from path. Check if the file exists and the header is valid.");

        // 2. Facility Retrieval
        let stream_handle: EventsStreamFacilityHandle = device
            .get_facility(FacilityType::EventsStreamFacility)
            .expect("EventsStreamFacility was not registered")
            .try_into()
            .unwrap();

        let mut stream = stream_handle.write().unwrap();

        let decoder_handle: EventsStreamDecoderFacilityHandle = device
            .get_facility(FacilityType::EventsStreamDecoderFacility)
            .expect("EventsStreamDecoderFacility was not registered")
            .try_into()
            .unwrap();

        let mut decoder = decoder_handle.write().unwrap();

        // Optional: If you implement a way to retrieve the dispatcher from the trait object
        // let cd_receiver = decoder.subscribe_cd(100);

        // 3. Start Stream
        stream.start().expect("Failed to start stream");

        let mut total_bytes_read = 0;
        let mut chunks_processed = 0;

        // 4. Read and Decode Loop
        loop {
            match stream.wait_next_buffer() {
                Ok((buffer, size)) => {
                    total_bytes_read += size;
                    chunks_processed += 1;

                    // Ensure the decoder processes the raw bytes without returning an Err
                    decoder.decode(buffer)?;

                    // Optional: Assert event dispatching
                    // while let Ok(events) = cd_receiver.try_recv() {
                    //     assert!(!events.is_empty());
                    // }
                }
                Err(FacilityError::Stream(StreamError::EndOfFile)) => {
                    break; // Successfully reached the end of the file
                }
                Err(e) => {
                    panic!("Unexpected stream error: {:?}", e);
                }
            }
        }

        // 5. Cleanup
        stream.stop().expect("Failed to stop stream");

        // 6. Validation
        assert!(
            total_bytes_read > 0,
            "Stream completed but zero bytes were read."
        );
        assert!(chunks_processed > 0, "No chunks were processed.");

        println!(
            "Successfully parsed {} bytes across {} chunks.",
            total_bytes_read, chunks_processed
        );

        Ok(())
    }
}
