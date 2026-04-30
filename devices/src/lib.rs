pub mod header;
pub mod types;

use crate::header::{Header, sensor_info_from_header};
use crate::types::{DeviceFileError, FileFormat};
use crossbeam::channel::Receiver;
use macros::pack_facility;
use openeb_core::hal::decoders::evt3::Evt3Decoder;
use openeb_core::hal::decoders::raw_fmt_decoder::RawFormatDecoder;
use openeb_core::hal::device::device::Device;
use openeb_core::hal::errors::{SharedError, StreamError};
use openeb_core::hal::facilities::{
    BaseDecoderFacility, ConnectionType, EventDecoderFacility, EventsStreamDecoderFacility,
    EventsStreamFacility, FacilityError, FacilityHandle, FacilityResult, FacilityType,
    GeometryFacility, HWIdentificationFacility, SensorInfo, SystemInfo,
};
use openeb_core::hal::types::EventCD;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, RwLock};
use utilities::buffer::PooledBuffer;

pub struct RawFileReader {
    header: Arc<Header>,
    facilities: HashMap<FacilityType, FacilityHandle>,
}

impl RawFileReader {
    fn new_from_path(path: &str) -> Result<Self, DeviceFileError> {
        let mut file = std::fs::File::open(path)?;

        // 1. Create the reader and parse the header
        let mut reader = std::io::BufReader::new(&mut file);
        let header = Header::parse(&mut reader)?;

        // 2. Capture the exact logical byte offset where the header ends
        let header_end_pos = reader.stream_position()?;

        // 3. Drop the BufReader to release the mutable borrow on `file`
        drop(reader);

        // 4. Force the OS file descriptor back to the start of the binary payload
        file.seek(SeekFrom::Start(header_end_pos))?;

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

        // The file is now perfectly aligned for the stream facility
        let stream = RREventStream::new(file);
        device.register_facility(
            FacilityType::EventsStreamFacility,
            pack_facility!(mut EventsStreamFacility, stream),
        );

        let decoder = RREventStreamDecoder::new(&header_arc.clone(), true); // Assuming do_time_shift = true
        device.register_facility(
            FacilityType::EventsStreamDecoderFacility,
            pack_facility!(mut EventsStreamDecoderFacility, decoder.clone()),
        );
        device.register_facility(
            FacilityType::EventDecoderFacility,
            pack_facility!(mut EventDecoderFacility, decoder),
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

#[derive(Clone)]
pub struct RREventStreamDecoder {
    // Requires Send + Sync if the device will be shared across thread boundaries
    inner: Arc<RwLock<Box<dyn RawFormatDecoder + Send + Sync>>>,
    pub event_format: FileFormat,
}

impl RREventStreamDecoder {
    pub fn new(header: &Header, do_time_shift: bool) -> Self {
        let decoder: Box<dyn RawFormatDecoder + Send + Sync> = match header.format {
            FileFormat::EVT3 => Box::new(Evt3Decoder::new(
                header.width as u16,
                header.height as u16,
                do_time_shift,
            )),
            FileFormat::EVT2 => todo!("Implement EVT2 Decoder"),
            FileFormat::DAT => todo!("Implement DAT Decoder"),
            FileFormat::HDF5 => todo!("Implement HDF5 Decoder"),
            FileFormat::UNKNOWN => unimplemented!("Cannot construct decoder for UNKNOWN format"),
        };

        Self {
            event_format: header.format,
            inner: Arc::new(RwLock::new(decoder)),
        }
    }
}

impl EventDecoderFacility for RREventStreamDecoder {
    fn subscribe_to_event_buffer(&mut self) -> Receiver<Arc<PooledBuffer<EventCD>>> {
        self.inner.write().unwrap().subscribe_to_event_buffer()
    }

    fn add_event_buffer(&mut self, range: Arc<PooledBuffer<EventCD>>) {
        self.inner.write().unwrap().add_event_buffer(range)
    }
}

impl EventsStreamDecoderFacility for RREventStreamDecoder {
    fn decode(&mut self, raw_data: &[u8]) -> FacilityResult<()> {
        self.inner.write().unwrap().decode(raw_data)
    }

    fn get_last_timestamp(&self) -> usize {
        self.inner.read().unwrap().get_last_timestamp()
    }

    fn get_timestamp_shift(&self) -> Option<usize> {
        self.inner.read().unwrap().get_timestamp_shift()
    }

    fn is_time_shifting_enabled(&self) -> bool {
        self.inner.read().unwrap().is_time_shifting_enabled()
    }

    fn reset_last_timestamp(&mut self, timestamp: usize) {
        self.inner.write().unwrap().reset_last_timestamp(timestamp)
    }

    fn reset_timestamp_shift(&mut self, shift: usize) {
        self.inner.write().unwrap().reset_timestamp_shift(shift)
    }

    fn is_decoded_event_stream_indexable(&self) -> bool {
        self.inner
            .read()
            .unwrap()
            .is_decoded_event_stream_indexable()
    }
}

impl BaseDecoderFacility for RREventStreamDecoder {
    fn subscribe_to_protocol_violation(&mut self) -> Receiver<SharedError> {
        self.inner
            .write()
            .unwrap()
            .subscribe_to_protocol_violation()
    }

    fn get_raw_event_size_bytes(&self) -> FacilityResult<u8> {
        self.inner.read().unwrap().get_raw_event_size_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*; // Adjust imports based on your file structure
    use openeb_core::hal::errors::StreamError;
    use openeb_core::hal::facilities::{
        EventDecoderFacilityHandle, EventsStreamDecoderFacilityHandle, EventsStreamFacilityHandle,
        FacilityError, FacilityType,
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

        let event_decoder_handle: EventDecoderFacilityHandle = device
            .get_facility(FacilityType::EventDecoderFacility)
            .expect("EventDecoderFacility was not registered")
            .try_into()
            .unwrap();

        let mut decoder = decoder_handle.write().unwrap();
        let mut event_decoder = event_decoder_handle.write().unwrap();

        // Optional: If you implement a way to retrieve the dispatcher from the trait object
        let cd_receiver = event_decoder.subscribe_to_event_buffer();

        // 3. Start Stream
        stream.start().expect("Failed to start stream");

        let mut total_bytes_read = 0;
        let mut chunks_processed = 0;

        // 4. Read and Decode Loop
        loop {
            match stream.wait_next_buffer() {
                Ok((buffer, size)) => {
                    // Decode the raw bytes
                    chunks_processed += 1;
                    total_bytes_read += size;
                    decoder.decode(buffer)?;

                    // Drain the receiver channel without blocking
                    while let Ok(event_batch) = cd_receiver.try_recv() {
                        // event_batch is of type Arc<PooledBuffer<EventCD>>
                        // Rust's auto-deref allows direct iteration over the underlying Vec
                        for event in event_batch.iter() {
                            // Execute operations on the EventCD struct
                            // e.g., accessing event.x, event.y, event.p, event.t
                            dbg!("Event: {}", event);
                        }

                        // Memory Recycling:
                        // When event_batch goes out of scope here, the Arc reference count decrements.
                        // If it reaches 0, the PooledBuffer's Drop implementation executes,
                        // clearing the vector and returning the capacity to the object pool.
                    }
                }
                Err(FacilityError::Stream(StreamError::EndOfFile)) => {
                    break;
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
