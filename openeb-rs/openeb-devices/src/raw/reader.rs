use crate::EventWindowIterator;
use crate::raw::decoder::RREventStreamDecoder;
use crate::raw::device::RawFileHandler;
use crate::raw::stream::RREventStream;
use crate::raw::{IterUnconfigured, index};
use crate::types::{DeviceFileError, FileIndex};
use crossbeam::channel::{Receiver, Sender};
use num_traits::ToPrimitive;
use openeb_core::hal::device::device::Device;
use openeb_core::hal::facilities::{
    EventDecoderFacilityHandle, EventsStreamDecoderFacility, EventsStreamDecoderFacilityHandle,
    EventsStreamFacility, EventsStreamFacilityHandle, FacilityError, FacilityType,
};
use openeb_core::hal::types::{EventCD, EventExtTrigger};
use std::marker::PhantomData;
use std::sync::{Arc, RwLock};
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

    pub fn as_windows(
        &mut self,
    ) -> Result<EventWindowIterator<BUFFER_SIZE, IterUnconfigured>, DeviceFileError> {
        let receiver = self.cd_receiver()?;
        let decoder_handle = self.get_decoder_handle()?;
        let stream_handle = self.get_stream_handle()?;
        let shape = self._device.shape();
        Ok(EventWindowIterator::<BUFFER_SIZE>::new(
            receiver,
            shape,
            stream_handle,
            decoder_handle,
        ))
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
