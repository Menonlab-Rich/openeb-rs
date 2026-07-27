use crate::header::Header;
use crate::raw::decoder::RREventStreamDecoder;
use crate::raw::facilities::{RawReaderGeometry, RawReaderHWIdentification, RawReaderROI};
use crate::raw::stream::RREventStream;
use crate::types::DeviceFileError;
use macros::pack_facility;
use openeb_core::hal::device::device::Device;
use openeb_core::hal::facilities::{FacilityHandle, FacilityType};
use std::collections::HashMap;
use std::io::{Seek, SeekFrom};
use std::sync::Arc;

pub(crate) struct RawFileHandler<const N: usize> {
    header: Arc<Header>,
    facilities: HashMap<FacilityType, FacilityHandle>,
    header_end_pos: u64,
}

impl<const N: usize> RawFileHandler<N> {
    pub(crate) fn new() -> Self {
        Self {
            header: Arc::new(Header::default()),
            facilities: HashMap::default(),
            header_end_pos: u64::default(),
        }
    }

    pub(crate) fn shape(&self) -> (u32, u32) {
        (self.header.height, self.header.width)
    }

    pub(crate) fn new_from_path(path: &str) -> Result<Self, DeviceFileError> {
        let mut file = std::fs::File::open(path)?;

        let mut reader = std::io::BufReader::new(&mut file);
        let header = Header::parse(&mut reader)?;
        let header_end_pos = reader.stream_position()?;

        drop(reader);
        file.seek(SeekFrom::Start(header_end_pos))?;

        let header = Arc::new(header);
        let mut device = RawFileHandler {
            header: header.clone(),
            facilities: HashMap::new(),
            header_end_pos,
        };

        let geometry =
            RawReaderGeometry::new(device.header.width as i32, device.header.height as i32);
        device.register_facility(
            FacilityType::GeometryFacility,
            pack_facility!(ro GeometryFacility, geometry),
        );

        let hw_identification = RawReaderHWIdentification::new(header.clone());
        device.register_facility(
            FacilityType::HWIdentificationFacility,
            pack_facility!(ro HWIdentificationFacility, hw_identification),
        );

        let stream = RREventStream::<N>::new(file);
        device.register_facility(
            FacilityType::EventsStreamFacility,
            pack_facility!(mut EventsStreamFacility, stream),
        );

        let decoder = RREventStreamDecoder::new(&header, true);
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

    pub(crate) fn try_open(&mut self, path: &str) -> Result<(), DeviceFileError> {
        let mut file = std::fs::File::open(path)?;

        let mut reader = std::io::BufReader::new(&mut file);
        let header = Header::parse(&mut reader)?;
        let header_end_pos = reader.stream_position()?;

        drop(reader);
        file.seek(SeekFrom::Start(header_end_pos))?;

        let header = Arc::new(header);
        self.header = header.clone();
        self.header_end_pos = header_end_pos;
        self.facilities.clear();

        let geometry = RawReaderGeometry::new(header.width as i32, header.height as i32);
        self.register_facility(
            FacilityType::GeometryFacility,
            pack_facility!(ro GeometryFacility, geometry),
        );

        let hw_identification = RawReaderHWIdentification::new(header.clone());
        self.register_facility(
            FacilityType::HWIdentificationFacility,
            pack_facility!(ro HWIdentificationFacility, hw_identification),
        );

        let stream = RREventStream::<N>::new(file);
        self.register_facility(
            FacilityType::EventsStreamFacility,
            pack_facility!(mut EventsStreamFacility, stream),
        );

        let roi = RawReaderROI::default();
        self.register_facility(
            FacilityType::ROIFacility,
            pack_facility!(mut ROIFacility, roi),
        );

        let decoder = RREventStreamDecoder::new(&header, true);

        self.register_facility(
            FacilityType::EventsStreamDecoderFacility,
            pack_facility!(mut EventsStreamDecoderFacility, decoder.clone()),
        );
        self.register_facility(
            FacilityType::EventDecoderFacility,
            pack_facility!(mut EventDecoderFacility, decoder),
        );

        Ok(())
    }

    pub(crate) fn header_end_pos(&self) -> u64 {
        self.header_end_pos
    }
}

impl<const N: usize> Device for RawFileHandler<N> {
    fn get_facility(&self, facility_type: FacilityType) -> Option<FacilityHandle> {
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
