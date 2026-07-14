use crate::raw::decoder::RREventStreamDecoder;
use crate::raw::device::RawFileHandler;
use crate::raw::index;
use crate::raw::stream::RREventStream;
use crate::types::{DeviceFileError, FileIndex};
use crossbeam::channel::Receiver;
use num_traits::ToPrimitive;
use openeb_core::hal::device::device::Device;
use openeb_core::hal::facilities::{
    EventDecoderFacilityHandle, EventsStreamDecoderFacilityHandle, EventsStreamFacility,
    EventsStreamFacilityHandle, FacilityError, FacilityType,
};
use openeb_core::hal::types::EventCD;
use std::sync::Arc;
use utilities::buffer::PooledBuffer;

pub struct RawFileReader<const BUFFER_SIZE: usize> {
    _device: RawFileHandler<BUFFER_SIZE>,
    stream_handle: EventsStreamFacilityHandle,
    decoder_handle: EventsStreamDecoderFacilityHandle,
    _event_decoder_handle: EventDecoderFacilityHandle,
    index: Option<Arc<FileIndex>>,
}

impl<const BUFFER_SIZE: usize> RawFileReader<BUFFER_SIZE> {
    pub fn try_open(file_path: &str, do_index: bool) -> Result<Self, DeviceFileError> {
        let device = RawFileHandler::<BUFFER_SIZE>::new_from_path(file_path)?;

        let index = match do_index {
            true => Some(Arc::new(index::build_index(
                file_path,
                device.header_end_pos(),
                1024 * 1024,
            )?)),

            false => None,
        };

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

        Ok(Self {
            _device: device,
            stream_handle,
            decoder_handle,
            _event_decoder_handle: event_decoder_handle,
            index,
        })
    }

    pub fn seek(&mut self, ts: u32) -> Result<(), DeviceFileError> {
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

        let stream = crate::facility_downcast_mut!(stream_facility, RREventStream<BUFFER_SIZE>)?;
        let decoder = crate::facility_downcast_mut!(decoder_facility, RREventStreamDecoder)?;

        index::seek_to_timestamp(
            index.as_ref(),
            ts.to_usize().expect("Failed to convert timestamp"),
            stream,
            decoder,
        )
    }

    pub fn cd_receiver(&mut self) -> Result<Receiver<Arc<PooledBuffer<EventCD>>>, DeviceFileError> {
        let mut stream_facility = self
            .stream_handle
            .as_ref()
            .try_write()
            .map_err(|_| DeviceFileError::WriteLockError)?;

        let mut event_decoder_facility = self
            ._event_decoder_handle
            .as_ref()
            .try_write()
            .map_err(|_| DeviceFileError::WriteLockError)?;

        let stream = crate::facility_downcast_mut!(stream_facility, RREventStream<BUFFER_SIZE>)?;

        let cd_receiver = event_decoder_facility.subscribe_to_event_buffer();
        stream.start()?;
        Ok(cd_receiver)
    }

    pub fn load_batch(&mut self) -> Result<(), DeviceFileError> {
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

        let stream = crate::facility_downcast_mut!(stream_facility, RREventStream<BUFFER_SIZE>)?;
        let (buffer, _) = stream.poll_buffer()?;
        decoder_facility.decode(buffer)?;
        Ok(())
    }

    pub fn as_windows(&mut self) -> Result<EventWindowIterator, DeviceFileError> {
        let receiver = self.cd_receiver()?;
        Ok(EventWindowIterator::new(receiver))
    }
}

pub struct EventWindowIterator {
    receiver: Receiver<Arc<PooledBuffer<EventCD>>>,
    // Holds leftover events extracted from PooledBuffers that haven't been consumed yet
    internal_buffer: std::collections::VecDeque<EventCD>,
    // Tracks the current temporal baseline for slicing fixed delta-t windows
    current_timestamp: Option<u64>,
}

impl EventWindowIterator {
    pub fn new(receiver: Receiver<Arc<PooledBuffer<EventCD>>>) -> Self {
        Self {
            receiver,
            internal_buffer: std::collections::VecDeque::new(),
            current_timestamp: None,
        }
    }

    /// Fills the internal queue from the channel if it runs empty
    fn replenish_buffer(&mut self) -> Result<(), DeviceFileError> {
        if self.internal_buffer.is_empty() {
            // Block until a new chunk arrives or channel disconnects
            if let Ok(pooled_buffer) = self.receiver.recv() {
                // Assuming PooledBuffer implements AsRef<[EventCD]> or can be iterated over
                self.internal_buffer
                    .extend(pooled_buffer.as_ref().iter().cloned());
            }
        }
        Ok(())
    }

    /// Pulls events until the specified count is hit
    pub fn next_batch(&mut self, size: usize) -> Result<Vec<EventCD>, DeviceFileError> {
        let mut batch = Vec::with_capacity(size);

        while batch.len() < size {
            if self.internal_buffer.is_empty() {
                self.replenish_buffer()?;
                if self.internal_buffer.is_empty() {
                    break; // Stream ended
                }
            }

            if let Some(event) = self.internal_buffer.pop_front() {
                // Establish initial time anchoring if needed
                if self.current_timestamp.is_none() {
                    self.current_timestamp = Some(event.t as u64);
                }
                batch.push(event);
            }
        }

        Ok(batch)
    }

    /// Pulls all events falling within a specific delta-t window
    pub fn next_delta(&mut self, dt: u64) -> Result<Vec<EventCD>, DeviceFileError> {
        // 1. Ensure we have at least one event to establish a time anchor
        if self.internal_buffer.is_empty() {
            self.replenish_buffer()?;
        }

        let start_ts = match self.current_timestamp {
            Some(ts) => ts,
            None => {
                if let Some(first_ev) = self.internal_buffer.front() {
                    let ts = first_ev.t as u64;
                    self.current_timestamp = Some(ts);
                    ts
                } else {
                    return Ok(Vec::new()); // No data available
                }
            }
        };

        let end_ts = start_ts + dt;
        let mut window_events = Vec::new();

        loop {
            if self.internal_buffer.is_empty() {
                self.replenish_buffer()?;
                if self.internal_buffer.is_empty() {
                    break; // Stream ended
                }
            }

            // Peek at the next item to check timestamp bounds
            if let Some(event) = self.internal_buffer.front() {
                if (event.t as u64) < end_ts {
                    window_events.push(self.internal_buffer.pop_front().unwrap());
                } else {
                    // Reached the next window frame boundary
                    break;
                }
            }
        }

        // Slide window baseline forward
        self.current_timestamp = Some(end_ts);
        Ok(window_events)
    }
}
