use crate::raw::decoder::RREventStreamDecoder;
use crate::raw::device::RawFileHandler;
use crate::raw::index;
use crate::raw::stream::RREventStream;
use crate::types::{DeviceFileError, FileIndex};
use num_traits::ToPrimitive;
use openeb_core::hal::device::device::Device;
use openeb_core::hal::facilities::{
    EventDecoderFacilityHandle, EventsStreamDecoderFacilityHandle, EventsStreamFacilityHandle,
    FacilityError, FacilityType,
};
use std::sync::Arc;

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

    pub fn events_iter_dt(&mut self) {}
}
