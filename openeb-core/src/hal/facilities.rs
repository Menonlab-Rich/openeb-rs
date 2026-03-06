use crate::hal::errors::{DecoderProtocolViolation, HalResult};
use crate::hal::types::{Cb, CbRo, EventSlice, PixelMask, Region};
use crate::property;
use std::any::Any;
use std::collections::{HashMap, HashSet};

pub trait Facility: Any + Send + Sync {
    fn registration_info(&self) -> HashSet<usize>;
    fn as_any(&self) -> &dyn Any;
}

pub trait RegisterableFacility: Facility {
    fn class_registration_info(&self) -> usize;
}

// --- Supporting Types ---

#[derive(Debug)]
pub enum AntiFlickerMode {
    BandPass,
    BandStop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraSyncMode {
    Standalone,
    Master,
    Slave,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    Usb,
    Mipi,
    Proprietary,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct SensorInfo {
    pub name: String,
    pub integrator: String,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub serial_number: String,
    pub firmware_version: String,
}

// --- Facilities ---

pub trait AntiFlickerFacility: Facility {
    property! {
        frequency: u32;
        frequency_band: (u32, u32);
        low_frequency: u32;
        min_supported_frequency: u32;
        max_supported_frequency: u32;
        filtering_mode: u32;
        duty_cycle: f32;
        min_supported_duty_cycle: f32;
        max_supported_duty_cycle: f32;
        start_threshold: u32;
        stop_threshold: u32;
        start_stop_threshold: (u32, u32);
        min_supported_start_threshold: u32;
        max_supported_start_threshold: u32;
        min_supported_stop_threshold: u32;
        max_supported_stop_threshold: u32;
        enabled: bool;
    }
}

pub trait BaseDecoderFacility: Facility {
    fn add_protocol_violation_callback(
        &mut self,
        cb: Cb<DecoderProtocolViolation>,
    ) -> HalResult<usize>;

    // consume the ID because once its removed, you shouldn't use it again.
    fn remove_protocol_violation_callback(&mut self, cb_id: usize) -> HalResult<()>;

    property! {
        raw_event_size_bytes: u8;
    }
}

pub trait DecoderFacility<T>: BaseDecoderFacility {
    fn decode(&mut self, raw_data: &[u8]) -> HalResult<()>;
    fn add_decode_callback(&mut self, cb: Cb<&[T]>) -> HalResult<usize>;

    // consume the ID because once removed, you shouldn't use it again.
    fn remove_decode_callback(&mut self, cb_id: usize) -> HalResult<()>;
}

pub trait BufferDecoderFacility<T>: BaseDecoderFacility {
    fn decode_to_buffer(&mut self, raw_data: &[u8], output: &mut Vec<T>) -> HalResult<()>;
}

pub trait CameraSyncFacility: Facility {
    property! {
        mode: CameraSyncMode;
    }
}

pub trait DigitalCropFacility: Facility {
    property! {
        enabled: bool;
        window_region: Region;
    }
}

pub trait DigitalEventMaskFacility: Facility {
    property! {
        masks: Vec<PixelMask>;
    }
}

pub trait ERCModuleFacility: Facility {
    property! {
        enabled: bool;
        cd_event_rate: u32;
        max_supported_cd_event_rate: u32;
        min_supported_cd_event_rate: u32;
        count_period: u32;
        cd_event_count: u32;
        min_supported_cd_event_count: u32;
        max_supported_cd_event_count: u32;
    }

    fn erc_from_file(&mut self, path: &str) -> HalResult<()>;
}

pub trait EventDecoderFacility: Facility {
    fn add_event_buffer_callback(&mut self, callback: CbRo<EventSlice>) -> HalResult<usize>;
    fn remove_event_buffer_callback(&mut self, id: usize) -> HalResult<()>;
    fn add_event_buffer(&mut self, range: EventSlice) -> HalResult<()>;
}

pub trait EventFrameDecoderFacility: Facility {
    type FrameType;
    property! {
        width: u32;
        height: u32;
    }

    fn add_event_frame_cb(&mut self, callback: CbRo<&Self::FrameType>) -> HalResult<usize>;
}

pub trait EventRateActivityFilterModuleFacility: Facility {
    property! {
        enabled: bool;
        thresholds: (u32, u32);
    }
}

pub trait EventsStreamFacility: Facility {
    fn start(&mut self) -> HalResult<()>;
    fn stop(&mut self) -> HalResult<()>;
    fn poll_buffer(&mut self) -> HalResult<Vec<u8>>;
    fn wait_next_buffer(&mut self) -> HalResult<Vec<u8>>;
}

pub trait GeometryFacility: Facility {
    fn get_width(&self) -> i32;
    fn get_height(&self) -> i32;
}

pub trait HALSoftwareInfoFacility: Facility {
    fn get_version(&self) -> String;
}

pub trait HWIdentificationFacility: Facility {
    fn get_serial(&self) -> HalResult<String>;
    fn get_system_id(&self) -> HalResult<i64>;
    fn get_sensor_info(&self) -> HalResult<SensorInfo>;
    fn get_system_info(&self) -> HalResult<SystemInfo>;
    fn get_connection_type(&self) -> HalResult<ConnectionType>;
    fn get_available_data_encoding_formats(&self) -> HalResult<Vec<String>>;
    fn get_current_data_encoding_format(&self) -> HalResult<String>;
}

pub trait HWRegisterFacility: Facility {
    fn read_register(&self, address: u32) -> HalResult<u32>;
    fn write_register(&mut self, address: u32, value: u32) -> HalResult<()>;
}

pub trait LLBiasesFacility: Facility {
    fn set(&mut self, bias_name: &str, bias_value: i32) -> HalResult<()>;
    fn get(&self, bias_name: &str) -> HalResult<i32>;
    fn get_all_biases(&self) -> HalResult<HashMap<String, i32>>;
}

pub trait MonitoringFacility: Facility {
    fn get_temperature(&self) -> HalResult<i32>;
    fn get_illumination(&self) -> HalResult<i32>;
}

pub trait PluginSoftwareInfoFacility: Facility {
    fn get_plugin_name(&self) -> String;
    fn get_version(&self) -> String;
}

pub trait ROIFacility: Facility {
    property! {
        enabled: bool;
    }

    fn set_roi(&mut self, region: Region) -> HalResult<()>;
    fn set_rois(&mut self, regions: &[Region]) -> HalResult<()>;
}

pub trait ROIPixelMaskFacility: Facility {
    property! {
        pixel_masks: Vec<PixelMask>;
    }
}

pub trait TriggerInFacility: Facility {
    fn enable(&mut self, channel: u32) -> HalResult<()>;
    fn disable(&mut self, channel: u32) -> HalResult<()>;
}

pub trait TriggerOutFacility: Facility {
    fn enable(&mut self) -> HalResult<()>;
    fn disable(&mut self) -> HalResult<()>;
    fn set_period(&mut self, period_us: u32) -> HalResult<()>;
    fn set_duty_cycle(&mut self, duty_cycle: f64) -> HalResult<()>;
}
