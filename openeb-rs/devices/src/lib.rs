pub mod header;
pub mod types;

use crate::header::{Header, sensor_info_from_header};
use crate::types::{DeviceFileError, FileFormat, FileIndex, IndexMarker, RawEventBuffer};
use crossbeam::channel::Receiver;
use macros::pack_facility;
use num_traits::ToPrimitive;
use openeb_core::hal::decoders::evt3::Evt3Decoder;
use openeb_core::hal::decoders::raw_fmt_decoder::RawFormatDecoder;
use openeb_core::hal::device::device::Device;
use openeb_core::hal::errors::{SharedError, StreamError};
use openeb_core::hal::facilities::{
    BaseDecoderFacility, ConnectionType, EventDecoderFacility, EventDecoderFacilityHandle,
    EventsStreamDecoderFacility, EventsStreamDecoderFacilityHandle, EventsStreamFacility,
    EventsStreamFacilityHandle, FacilityError, FacilityHandle, FacilityResult, FacilityType,
    GeometryFacility, HWIdentificationFacility, SensorInfo, SystemInfo,
};
use openeb_core::hal::types::EventCD;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, RwLock};
use utilities::buffer::PooledBuffer;

struct RawFileHandler<const N: usize> {
    header: Arc<Header>,
    facilities: HashMap<FacilityType, FacilityHandle>,
    header_end_pos: u64,
}

impl<const N: usize> RawFileHandler<N> {
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
        let mut device = RawFileHandler {
            header: header_arc.clone(),
            facilities: HashMap::new(),
            header_end_pos,
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
        let stream = RREventStream::<N>::new(file);
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

impl<const N: usize> RawFileHandler<N> {
    /// Scans the binary payload sequentially to build a time-to-byte-offset map.
    /// `chunk_size_bytes` controls how granular the index is (e.g., 512 * 1024 for 512KB chunks).
    pub fn build_index(
        path: &str,
        header_end_pos: u64,
        chunk_size_bytes: u64,
    ) -> Result<FileIndex, DeviceFileError> {
        let mut file = std::fs::File::open(path)?;
        file.seek(SeekFrom::Start(header_end_pos))?;

        let mut decoder = Evt3Decoder::new(640, 480, true);
        let mut markers = Vec::new();

        let mut buffer = vec![0u8; 65536]; // 64KB sequential read block
        let mut current_byte_offset = header_end_pos;
        let mut last_marked_offset = header_end_pos;

        // Insert baseline start marker
        markers.push(IndexMarker {
            byte_offset: header_end_pos,
            timestamp: 0,
            state: decoder.get_timing_state(),
        });

        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            // Parse timing words out of the buffer chunk
            let words = buffer[..bytes_read].chunks_exact(2);
            for chunk in words {
                let word = u16::from_le_bytes([chunk[0], chunk[1]]);

                // We replicate ONLY the timing adjustments of process_word here for extreme speed
                let msb = (word >> 12) as u8;
                match msb {
                    0b0110 => {
                        // TimeLow
                        decoder._set_time_low((word & 0x0FFF).into());
                        let _ = decoder.current_timestamp();
                    }
                    0b1000 => {
                        // TimeHigh
                        decoder._set_time_high((word & 0x0FFF).into());
                        let _ = decoder.current_timestamp();
                    }
                    _ => {} // Ignore spatial/trigger data during indexing pass
                }
            }

            current_byte_offset += bytes_read as u64;

            // Whenever we cross our chunk threshold, drop an index marker boundary
            if current_byte_offset - last_marked_offset >= chunk_size_bytes {
                markers.push(IndexMarker {
                    byte_offset: current_byte_offset,
                    timestamp: decoder.get_last_timestamp(),
                    state: decoder.get_timing_state(),
                });
                last_marked_offset = current_byte_offset;
            }
        }

        Ok(FileIndex { markers })
    }
}

// Corrected Device Trait Implementation
impl<const N: usize> Device for RawFileHandler<N> {
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

struct RREventStream<const N: usize> {
    file: File,
    buffer: RawEventBuffer<N>,
    eof: bool,
}

impl<const N: usize> RREventStream<N> {
    fn new(file: File) -> Self {
        RREventStream {
            file,
            buffer: RawEventBuffer::<N>::new(),
            eof: false,
        }
    }
}

impl<const N: usize> EventsStreamFacility for RREventStream<N> {
    fn start(&mut self) -> FacilityResult<()> {
        Ok(()) // nothing to start when reading a file
    }
    fn stop(&mut self) -> FacilityResult<()> {
        Ok(()) // Nothing to stop when reading a file
    }
    fn poll_buffer(&mut self) -> FacilityResult<(&[u8], usize)> {
        self.wait_next_buffer()
    }
    fn wait_next_buffer(&mut self) -> FacilityResult<(&[u8], usize)> {
        if self.eof {
            return Err(FacilityError::Stream(StreamError::EndOfFile));
        }

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

impl<const N: usize> RawFileHandler<N> {
    pub fn seek_to_timestamp(
        &mut self,
        index: &FileIndex,
        target_timestamp: usize,
        stream_facility: &mut RREventStream<N>,
        decoder_facility: &mut Evt3Decoder,
    ) -> Result<(), DeviceFileError> {
        // 1. Find nearest chunk boundary
        if let Some(marker) = index.find_closest_marker(target_timestamp) {
            // 2. Reposition OS file descriptor
            stream_facility
                .file
                .seek(SeekFrom::Start(marker.byte_offset))?;
            stream_facility.eof = false;

            // 3. Force-synchronize decoder clocks to match that file coordinate context
            decoder_facility.set_timing_state(marker.state);

            // 4. Fast-forward through remaining events inside this chunk until exactly matching the target timestamp
            // (Events prior to target_timestamp within this chunk are processed but discarded or filtered out)
        }
        Ok(())
    }
}

pub struct RawFileReader<const BUFFER_SIZE: usize> {
    device: RawFileHandler<BUFFER_SIZE>,
    stream_handle: EventsStreamFacilityHandle,
    decoder_handle: EventsStreamDecoderFacilityHandle,
    event_decoder_handle: EventDecoderFacilityHandle,
    index: Option<Arc<FileIndex>>,
}

impl<const BUFFER_SIZE: usize> RawFileReader<BUFFER_SIZE> {
    pub fn try_open(file_path: &str, do_index: bool) -> Result<Self, DeviceFileError> {
        let device = RawFileHandler::<BUFFER_SIZE>::new_from_path(file_path)?;

        let index = match do_index {
            true => Some(Arc::new(RawFileHandler::<131_072>::build_index(
                file_path,
                device.header_end_pos,
                1024 * 1024,
            )?)),

            false => None,
        };

        // 2. Facility Retrieval
        let stream_handle: EventsStreamFacilityHandle = device
            .get_facility(FacilityType::EventsStreamFacility)
            .ok_or(DeviceFileError::UnsupportedFacility(
                "EventsStreamFacility".to_string(),
            ))?
            .try_into()?;

        let decoder_handle: EventsStreamDecoderFacilityHandle = device
            .get_facility(FacilityType::EventsStreamDecoderFacility)
            .ok_or(DeviceFileError::UnsupportedFacility(
                "EventsStreamDecoderFacility".to_string(),
            ))?
            .try_into()?;

        let event_decoder_handle: EventDecoderFacilityHandle = device
            .get_facility(FacilityType::EventDecoderFacility)
            .ok_or(DeviceFileError::UnsupportedFacility(
                "EventDecoderFacility".to_string(),
            ))?
            .try_into()?;

        Ok(RawFileReader {
            device,
            stream_handle,
            decoder_handle,
            event_decoder_handle,
            index,
        })
    }

    pub fn seek(&mut self, ts: u32) -> Result<(), DeviceFileError> {
        // Acquire locks
        let index = self
            .index
            .clone()
            .ok_or(DeviceFileError::UnsupportedBehavior(
                "File must be indexed in order to use seek.".to_string(),
            ))?;
        let mut stream_facility = self
            .stream_handle
            .as_ref()
            .try_write()
            .map_err(|_| DeviceFileError::WriteLockError)?;
        let mut decoder_facility = self
            .decoder_handle
            .as_ref()
            .try_write()
            .map_err(|_| DeviceFileError::WriteLockError)?;

        let mut stream = stream_facility
            .as_any_mut()
            .downcast_mut::<RREventStream<BUFFER_SIZE>>()
            .ok_or(DeviceFileError::FacilityDowncastError(
                "EventsStreamFaciliy".to_string(),
                "RREventStream".to_string(),
            ))?;

        let mut decoder = decoder_facility
            .as_any_mut()
            .downcast_mut::<Evt3Decoder>()
            .ok_or(DeviceFileError::FacilityDowncastError(
                "EventsStreamDecoderFacility".to_string(),
                "RREventStreamDecoder".to_string(),
            ))?;

        self.device.seek_to_timestamp(
            index.as_ref(),
            ts.to_usize().expect("Failed to convert timestamp"),
            stream,
            decoder,
        )?;

        Ok(())
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
        let device = RawFileHandler::<131_072>::new_from_path(file_path.into_os_string().to_str().expect("A cargo manifest dir must be specified."))
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
