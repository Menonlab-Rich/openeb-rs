use crate::header::{Header, sensor_info_from_header};
use openeb_core::hal::facilities::{
    ConnectionType, FacilityResult, GeometryFacility, HWIdentificationFacility, SensorInfo,
    SystemInfo,
};
use std::sync::Arc;

pub(crate) struct RawReaderGeometry {
    width: i32,
    height: i32,
}

impl RawReaderGeometry {
    pub(crate) fn new(width: i32, height: i32) -> Self {
        Self { width, height }
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

pub(crate) struct RawReaderHWIdentification {
    header: Arc<Header>,
}

impl RawReaderHWIdentification {
    pub(crate) fn new(header: Arc<Header>) -> Self {
        Self { header }
    }
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
