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
use openeb_core::hal::types::{EventCD, EventExtTrigger};
use std::sync::Arc;
use utilities::buffer::PooledBuffer;

pub struct RawFileReader<const BUFFER_SIZE: usize> {
    _device: RawFileHandler<BUFFER_SIZE>,
    stream_handle: Option<EventsStreamFacilityHandle>,
    decoder_handle: Option<EventsStreamDecoderFacilityHandle>,
    event_decoder_handle: Option<EventDecoderFacilityHandle>,
    index: Option<Arc<FileIndex>>,
    initialized: bool,
}

impl<const BUFFER_SIZE: usize> RawFileReader<BUFFER_SIZE> {
    pub fn ready(&self) -> bool {
        self.stream_handle.is_some()
    }
}

impl<const BUFFER_SIZE: usize> RawFileReader<BUFFER_SIZE> {
    pub fn new() -> Self {
        let device = RawFileHandler::<BUFFER_SIZE>::new();
        Self {
            _device: device,
            stream_handle: None,
            decoder_handle: None,
            event_decoder_handle: None,
            index: None,
            initialized: false,
        }
    }
    pub fn try_from_file(file_path: &str, do_index: bool) -> Result<Self, DeviceFileError> {
        let device = RawFileHandler::<BUFFER_SIZE>::new_from_path(file_path)?;

        let index = match do_index {
            true => Some(Arc::new(index::build_index(
                file_path,
                device.header_end_pos(),
                1024 * 1024,
            )?)),

            false => None,
        };

        let stream_handle: Option<EventsStreamFacilityHandle> = Some(
            device
                .get_facility(FacilityType::EventsStreamFacility)
                .ok_or(DeviceFileError::UnsupportedFacility(
                    "EventsStreamFacility".to_string(),
                ))?
                .try_into()?,
        );

        let decoder_handle: Option<EventsStreamDecoderFacilityHandle> = Some(
            device
                .get_facility(FacilityType::EventsStreamDecoderFacility)
                .ok_or(DeviceFileError::UnsupportedFacility(
                    "EventsStreamDecoderFacility".to_string(),
                ))?
                .try_into()?,
        );

        let event_decoder_handle: Option<EventDecoderFacilityHandle> = Some(
            device
                .get_facility(FacilityType::EventDecoderFacility)
                .ok_or(DeviceFileError::UnsupportedFacility(
                    "EventDecoderFacility".to_string(),
                ))?
                .try_into()?,
        );

        Ok(Self {
            _device: device,
            stream_handle,
            decoder_handle,
            event_decoder_handle,
            index,
            initialized: true,
        })
    }

    pub fn try_open(&mut self, file_path: &str, do_index: bool) -> Result<(), DeviceFileError> {
        self._device.try_open(file_path)?;
        let device = &self._device;

        self.index = match do_index {
            true => Some(Arc::new(index::build_index(
                file_path,
                device.header_end_pos(),
                1024 * 1024,
            )?)),

            false => None,
        };

        self.stream_handle = Some(
            device
                .get_facility(FacilityType::EventsStreamFacility)
                .ok_or(DeviceFileError::UnsupportedFacility(
                    "EventsStreamFacility".to_string(),
                ))?
                .try_into()?,
        );

        self.decoder_handle = Some(
            device
                .get_facility(FacilityType::EventsStreamDecoderFacility)
                .ok_or(DeviceFileError::UnsupportedFacility(
                    "EventsStreamDecoderFacility".to_string(),
                ))?
                .try_into()?,
        );

        self.event_decoder_handle = Some(
            device
                .get_facility(FacilityType::EventDecoderFacility)
                .ok_or(DeviceFileError::UnsupportedFacility(
                    "EventDecoderFacility".to_string(),
                ))?
                .try_into()?,
        );

        self.initialized = true;

        Ok(())
    }

    pub fn seek(&mut self, ts: u32) -> Result<(), DeviceFileError> {
        if !self.initialized {
            return Err(DeviceFileError::NotInitialized);
        }
        let index = self
            .index
            .clone()
            .ok_or(DeviceFileError::UnsupportedBehavior(
                "File must be indexed in order to use seek.".to_string(),
            ))?;
        let stream_handle = self.get_stream_handle()?;
        let decoder_handle = self.get_decoder_handle()?;
        let mut stream_facility = stream_handle
            .as_ref()
            .try_write()
            .map_err(|_| DeviceFileError::WriteLockError)?;
        let mut decoder_facility = decoder_handle
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

    pub fn seek_to_next_ext(&mut self) -> Result<(), DeviceFileError> {
        let recv = self.ext_receiver()?;
        loop {
            let _ = self.load_batch(); // Load another batch
            match recv.try_recv() {
                Ok(evts) => self.seek(evts[0].t as u32),
                Err(err) => Err(err.into()),
            }?
        }
    }

    pub fn cd_receiver(&mut self) -> Result<Receiver<Arc<PooledBuffer<EventCD>>>, DeviceFileError> {
        let stream_handle = self.get_stream_handle()?;
        let event_decoder_handle = self.get_event_decoder_handle()?;
        let mut stream_facility = stream_handle
            .as_ref()
            .try_write()
            .map_err(|_| DeviceFileError::WriteLockError)?;

        let mut event_decoder_facility = event_decoder_handle
            .as_ref()
            .try_write()
            .map_err(|_| DeviceFileError::WriteLockError)?;

        let stream = crate::facility_downcast_mut!(stream_facility, RREventStream<BUFFER_SIZE>)?;

        let cd_receiver = event_decoder_facility.subscribe_to_cd_events();
        stream.start()?;
        Ok(cd_receiver)
    }

    pub fn ext_receiver(
        &mut self,
    ) -> Result<Receiver<Arc<PooledBuffer<EventExtTrigger>>>, DeviceFileError> {
        let stream_handle = self.get_stream_handle()?;
        let event_decoder_handle = self.get_event_decoder_handle()?;
        let mut stream_facility = stream_handle
            .as_ref()
            .try_write()
            .map_err(|_| DeviceFileError::WriteLockError)?;

        let mut ext_evt_decoder_facility = event_decoder_handle
            .as_ref()
            .try_write()
            .map_err(|_| DeviceFileError::WriteLockError)?;

        let stream = crate::facility_downcast_mut!(stream_facility, RREventStream<BUFFER_SIZE>)?;

        let ext_receiver = ext_evt_decoder_facility.subscribe_to_ext_events();
        stream.start()?;

        Ok(ext_receiver)
    }

    pub fn load_batch(&mut self) -> Result<(), DeviceFileError> {
        let stream_handle = self.get_stream_handle()?;
        let decoder_handle = self.get_decoder_handle()?;
        let mut stream_facility = stream_handle
            .as_ref()
            .try_write()
            .map_err(|_| DeviceFileError::WriteLockError)?;

        let mut decoder_facility = decoder_handle
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
        let shape = self._device.shape();
        Ok(EventWindowIterator::new(receiver, shape))
    }

    fn assert_initialized(&self) -> Result<(), DeviceFileError> {
        if !self.initialized {
            return Err(DeviceFileError::NotInitialized);
        }

        return Ok(());
    }

    fn get_stream_handle(&self) -> Result<EventsStreamFacilityHandle, DeviceFileError> {
        let _ = self.assert_initialized()?;
        Ok(self.stream_handle.clone().unwrap())
    }

    fn get_event_decoder_handle(&self) -> Result<EventDecoderFacilityHandle, DeviceFileError> {
        let _ = self.assert_initialized()?;
        Ok(self.event_decoder_handle.clone().unwrap())
    }

    fn get_decoder_handle(&self) -> Result<EventsStreamDecoderFacilityHandle, DeviceFileError> {
        let _ = self.assert_initialized()?;
        Ok(self.decoder_handle.clone().unwrap())
    }
}

pub struct EventWindowIterator {
    receiver: Receiver<Arc<PooledBuffer<EventCD>>>,
    // Holds leftover events extracted from PooledBuffers that haven't been consumed yet
    internal_buffer: std::collections::VecDeque<EventCD>,
    // Tracks the current temporal baseline for slicing fixed delta-t windows
    current_timestamp: Option<u64>,
    shape: (u32, u32),
}

impl EventWindowIterator {
    pub fn new(receiver: Receiver<Arc<PooledBuffer<EventCD>>>, shape: (u32, u32)) -> Self {
        Self {
            receiver,
            internal_buffer: std::collections::VecDeque::new(),
            current_timestamp: None,
            shape,
        }
    }

    pub fn shape(&self) -> (u32, u32) {
        self.shape
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
