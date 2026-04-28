use crate::hal::decoders::evt3::PooledBuffer;
use crate::hal::types::{EventCD, EventExtTrigger};

use crate::hal::errors::{
    DecoderError, DecoderProtocolViolation, HardwareError, ProcessingError, SharedError,
    StreamError,
};
use crate::hal::types::{Cb, CbRo, EventSlice, PixelMask, Region};
use crossbeam::channel::Receiver;
pub use macros::pack_facility;
use macros::property;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use thiserror::Error;

pub type GeometryFacilityHandle = Arc<dyn GeometryFacility + Send + Sync>;
pub type HALSoftwareInfoFacilityHandle = Arc<dyn HALSoftwareInfoFacility + Send + Sync>;
pub type HWIdentificationFacilityHandle = Arc<dyn HWIdentificationFacility + Send + Sync>;
pub type MonitoringFacilityHandle = Arc<dyn MonitoringFacility + Send + Sync>;
pub type PluginSoftwareInfoFacilityHandle = Arc<dyn PluginSoftwareInfoFacility + Send + Sync>;

pub type AntiFlickerFacilityHandle = Arc<RwLock<dyn AntiFlickerFacility + Send>>;
pub type BaseDecoderFacilityHandle = Arc<RwLock<dyn BaseDecoderFacility + Send>>;
pub type CameraSyncFacilityHandle = Arc<RwLock<dyn CameraSyncFacility + Send>>;
pub type DigitalCropFacilityHandle = Arc<RwLock<dyn DigitalCropFacility + Send>>;
pub type DigitalEventMaskFacilityHandle = Arc<RwLock<dyn DigitalEventMaskFacility + Send>>;
pub type ERCModuleFacilityHandle = Arc<RwLock<dyn ERCModuleFacility + Send>>;
pub type EventDecoderFacilityHandle = Arc<RwLock<dyn EventDecoderFacility + Send>>;

pub type EventsStreamDecoderFacilityHandle = Arc<RwLock<dyn EventsStreamDecoderFacility + Send>>;

pub type EventFrameDecoderRGBFacilityHandle =
    Arc<RwLock<dyn EventFrameDecoderFacility<FrameType = RGBFrameType> + Send>>;
pub type EventFrameDecoderMonoFacilityHandle =
    Arc<RwLock<dyn EventFrameDecoderFacility<FrameType = MonoFrameType> + Send>>;
pub type EventRateActivityFilterModuleFacilityHandle =
    Arc<RwLock<dyn EventRateActivityFilterModuleFacility + Send>>;
pub type EventTrailFilterModuleFacilityHandle =
    Arc<RwLock<dyn EventTrailFilterModuleFacility + Send>>;
pub type EventsStreamFacilityHandle = Arc<RwLock<dyn EventsStreamFacility + Send>>;
pub type HWRegisterFacilityHandle = Arc<RwLock<dyn HWRegisterFacility + Send>>;
pub type LLBiasesFacilityHandle = Arc<RwLock<dyn LLBiasesFacility + Send>>;
pub type ROIFacilityHandle = Arc<RwLock<dyn ROIFacility + Send>>;
pub type ROIPixelMaskFacilityHandle = Arc<RwLock<dyn ROIPixelMaskFacility + Send>>;
pub type TriggerInFacilityHandle = Arc<RwLock<dyn TriggerInFacility + Send>>;
pub type TriggerOutFacilityHandle = Arc<RwLock<dyn TriggerOutFacility + Send>>;

pub type CDDecoderFacilityHandle = Arc<RwLock<dyn DecoderFacility<EventCD> + Send>>;
pub type EXTDecoderFacilityHandle = Arc<RwLock<dyn DecoderFacility<EventExtTrigger> + Send>>;
pub type ERCDecoderFacilityHandle = Arc<RwLock<dyn DecoderFacility<EventERCCounter> + Send>>;

use std::convert::TryFrom;

#[derive(Error, Debug)]
#[error("Facility type mismatch: The requested facility type does not match the retrieved handle.")]
pub struct FacilityTypeMismatch;

macro_rules! impl_try_from_facility {
    (
        // Matches: Variant, Type Alias
        $( $variant:ident, $alias:ty ),* $(,)?
    ) => {
        $(
            impl TryFrom<FacilityHandle> for $alias {
                type Error = FacilityTypeMismatch;

                fn try_from(handle: FacilityHandle) -> Result<Self, Self::Error> {
                    if let FacilityHandle::$variant(h) = handle {
                        Ok(h)
                    } else {
                        Err(FacilityTypeMismatch)
                    }
                }
            }
        )*
    };
}

impl_try_from_facility! {
    // --- Immutable Facilities ---
    GeometryFacility, GeometryFacilityHandle,
    HALSoftwareInfoFacility, HALSoftwareInfoFacilityHandle,
    HWIdentificationFacility, HWIdentificationFacilityHandle,
    MonitoringFacility, MonitoringFacilityHandle,
    PluginSoftwareInfoFacility, PluginSoftwareInfoFacilityHandle,

    // --- Mutable Facilities ---
    AntiFlickerFacility, AntiFlickerFacilityHandle,
    BaseDecoderFacility, BaseDecoderFacilityHandle,
    CameraSyncFacility, CameraSyncFacilityHandle,
    DigitalCropFacility, DigitalCropFacilityHandle,
    DigitalEventMaskFacility, DigitalEventMaskFacilityHandle,
    ERCModuleFacility, ERCModuleFacilityHandle,
    EventDecoderFacility, EventDecoderFacilityHandle,
    RGBEventFrameDecoderFacility, EventFrameDecoderRGBFacilityHandle,
    MonoEventFrameDecoderFacility, EventFrameDecoderMonoFacilityHandle,
    EventRateActivityFilterModuleFacility, EventRateActivityFilterModuleFacilityHandle,
    EventTrailFilterModuleFacility, EventTrailFilterModuleFacilityHandle,
    EventsStreamFacility, EventsStreamFacilityHandle,
    HWRegisterFacility, HWRegisterFacilityHandle,
    LLBiasesFacility, LLBiasesFacilityHandle,
    ROIFacility, ROIFacilityHandle,
    ROIPixelMaskFacility, ROIPixelMaskFacilityHandle,
    TriggerInFacility, TriggerInFacilityHandle,
    TriggerOutFacility, TriggerOutFacilityHandle,
    EventsStreamDecoderFacility, EventsStreamDecoderFacilityHandle,

    // --- Monomorphized Generic Mutable Facilities ---
    CDDecoderFacility, CDDecoderFacilityHandle,
    ExtTriggerDecoderFacility, EXTDecoderFacilityHandle,
    ERCCounterDecoderFacility, ERCDecoderFacilityHandle,
}

// TODO! implement these types and move them to the correct file
pub struct EventERCCounter {}
pub struct RGBFrameType {}
pub struct MonoFrameType {}

#[derive(Error, Debug)]
pub enum FacilityError {
    #[error(transparent)]
    Decoder(#[from] DecoderError),
    #[error(transparent)]
    Hardware(#[from] HardwareError),
    #[error(transparent)]
    Stream(#[from] StreamError),
    #[error(transparent)]
    Processing(#[from] ProcessingError),
    #[error(transparent)]
    DecoderProtocol(#[from] DecoderProtocolViolation),
}

pub type FacilityResult<T> = Result<T, FacilityError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FacilityType {
    AntiFlickerFacility,
    BaseDecoderFacility,
    CameraSyncFacility,
    DecoderFacility,
    DigitalCropFacility,
    DigitalEventMaskFacility,
    ERCModuleFacility,
    CDDecoderFacility,
    TriggerEventDecoderFaciliy,
    ERCCounterDecoderFacility,
    RGBEventFrameDecoderFacility,
    MonoEventFrameDecoderFacility,
    EventRateActivityFilterModuleFacility,
    EventTrailFilterModuleFacility,
    EventsStreamFacility,
    EventsStreamDecoderFacility,
    GeometryFacility,
    HALSoftwareInfoFacility,
    HWIdentificationFacility,
    HWRegisterFacility,
    LLBiasesFacility,
    MonitoringFacility,
    PluginSoftwareInfoFacility,
    ROIFacility,
    ROIPixelMaskFacility,
    TriggerInFacility,
    TriggerOutFacility,
}

// The handle containing the exact trait object
#[derive(Clone)]
pub enum FacilityHandle {
    // --- Immutable Facilities (Read-Only across threads) ---
    GeometryFacility(GeometryFacilityHandle),
    HALSoftwareInfoFacility(HALSoftwareInfoFacilityHandle),
    HWIdentificationFacility(HWIdentificationFacilityHandle),
    MonitoringFacility(MonitoringFacilityHandle),
    PluginSoftwareInfoFacility(PluginSoftwareInfoFacilityHandle),

    // --- Mutable Facilities (Exclusive access required) ---
    AntiFlickerFacility(AntiFlickerFacilityHandle),
    BaseDecoderFacility(BaseDecoderFacilityHandle),
    CameraSyncFacility(CameraSyncFacilityHandle),
    DigitalCropFacility(DigitalCropFacilityHandle),
    DigitalEventMaskFacility(DigitalEventMaskFacilityHandle),
    ERCModuleFacility(ERCModuleFacilityHandle),
    EventDecoderFacility(EventDecoderFacilityHandle),
    RGBEventFrameDecoderFacility(EventFrameDecoderRGBFacilityHandle),
    MonoEventFrameDecoderFacility(EventFrameDecoderMonoFacilityHandle),
    EventRateActivityFilterModuleFacility(EventRateActivityFilterModuleFacilityHandle),
    EventTrailFilterModuleFacility(EventTrailFilterModuleFacilityHandle),
    EventsStreamFacility(EventsStreamFacilityHandle),
    EventsStreamDecoderFacility(EventsStreamDecoderFacilityHandle),
    HWRegisterFacility(HWRegisterFacilityHandle),
    LLBiasesFacility(LLBiasesFacilityHandle),
    ROIFacility(ROIFacilityHandle),
    ROIPixelMaskFacility(ROIPixelMaskFacilityHandle),
    TriggerInFacility(TriggerInFacilityHandle),
    TriggerOutFacility(TriggerOutFacilityHandle),

    // --- Monomorphized Generic Mutable Facilities ---
    CDDecoderFacility(CDDecoderFacilityHandle),
    ExtTriggerDecoderFacility(EXTDecoderFacilityHandle),
    ERCCounterDecoderFacility(ERCDecoderFacilityHandle),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrailFilterTypes {
    TRAIL,
    STCCutTrail,
    STCKeepTrail,
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
pub trait AntiFlickerFacility {
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

pub trait BaseDecoderFacility {
    fn subscribe_to_protocol_violation(&mut self) -> Receiver<SharedError>;

    property! {
        ro raw_event_size_bytes: u8;
    }
}

pub trait DecoderFacility<T>: BaseDecoderFacility {
    fn decode(&mut self, raw_data: &[u8]) -> FacilityResult<()>;
    fn add_decode_callback(&mut self, cb: Cb<&[T]>) -> FacilityResult<usize>;

    // consume the ID because once removed, you shouldn't use it again.
    fn remove_decode_callback(&mut self, cb_id: usize) -> FacilityResult<()>;
}

pub trait BufferDecoderFacility<T>: BaseDecoderFacility {
    fn decode_to_buffer(&mut self, raw_data: &[u8], output: &mut Vec<T>) -> FacilityResult<()>;
}

pub trait CameraSyncFacility {
    property! {
        mode: CameraSyncMode;
    }
}

pub trait DigitalCropFacility {
    property! {
        enabled: bool;
        window_region: Region;
    }
}

pub trait DigitalEventMaskFacility {
    property! {
        masks: Vec<PixelMask>;
    }
}

pub trait ERCModuleFacility {
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

    fn erc_from_file(&mut self, path: &str) -> FacilityResult<()>;
}

pub trait EventDecoderFacility {
    fn subscribe_to_event_buffer(&mut self) -> Receiver<Arc<PooledBuffer<EventCD>>>;
    fn add_event_buffer(&mut self, range: Arc<PooledBuffer<EventCD>>);
}

pub trait EventFrameDecoderFacility {
    type FrameType;
    property! {
        width: u32;
        height: u32;
    }

    fn add_event_frame_cb(&mut self, callback: CbRo<&Self::FrameType>) -> FacilityResult<usize>;
}

pub trait EventTrailFilterModuleFacility {
    property! {
        enabled: bool;
        filter_type: TrailFilterTypes;
        threshold: u32;
    }
    fn get_available_types(&self) -> &HashSet<TrailFilterTypes>;
    fn get_max_supported_threshold(&self) -> u32;
    fn get_min_supported_threshold(&self) -> u32;
}

pub trait EventRateActivityFilterModuleFacility {
    property! {
        enabled: bool;
        thresholds: (u32, u32);
    }
}

pub trait EventsStreamFacility {
    fn start(&mut self) -> FacilityResult<()>;
    fn stop(&mut self) -> FacilityResult<()>;
    fn poll_buffer(&mut self) -> FacilityResult<(&[u8], usize)>;
    fn wait_next_buffer(&mut self) -> FacilityResult<(&[u8], usize)>;
}

pub trait EventsStreamDecoderFacility: BaseDecoderFacility {
    /// Decodes raw data. Identifies the events in the buffer and dispatches them
    /// to the corresponding event decoders.
    ///
    /// Warning: It is mandatory to pass strictly consecutive buffers from the same source.
    fn decode(&mut self, raw_data: &[u8]) -> FacilityResult<()>;

    /// Gets the timestamp of the last event.
    fn get_last_timestamp(&self) -> usize;

    /// Retrieves the timestamp shift (timestamp of the first event in the stream).
    /// Returns `Some(shift)` if known, otherwise `None`.
    fn get_timestamp_shift(&self) -> Option<usize>;

    /// Returns true if time shifting is enabled.
    fn is_time_shifting_enabled(&self) -> bool;

    /// Resets the decoder last timestamp.
    ///
    /// If time shifting is enabled, `timestamp` must be in the shifted time reference.
    fn reset_last_timestamp(&mut self, timestamp: usize);

    /// Resets the decoder timestamp shift.
    ///
    /// If time shifting is disabled, this function should do nothing.
    fn reset_timestamp_shift(&mut self, shift: usize);

    /// Returns true if the decoded events stream can be indexed.
    fn is_decoded_event_stream_indexable(&self) -> bool;
}

pub trait GeometryFacility {
    fn get_width(&self) -> i32;
    fn get_height(&self) -> i32;
}

pub trait HALSoftwareInfoFacility {
    fn get_version(&self) -> String;
}

pub trait HWIdentificationFacility {
    fn get_serial(&self) -> FacilityResult<String>;
    fn get_system_id(&self) -> FacilityResult<i64>;
    fn get_sensor_info(&self) -> FacilityResult<SensorInfo>;
    fn get_system_info(&self) -> FacilityResult<SystemInfo>;
    fn get_connection_type(&self) -> FacilityResult<ConnectionType>;
    fn get_available_data_encoding_formats(&self) -> FacilityResult<Vec<String>>;
    fn get_current_data_encoding_format(&self) -> FacilityResult<String>;
}

pub trait HWRegisterFacility {
    fn read_register(&self, address: u32) -> FacilityResult<u32>;
    fn write_register(&mut self, address: u32, value: u32) -> FacilityResult<()>;
}

pub trait LLBiasesFacility {
    fn set(&mut self, bias_name: &str, bias_value: i32) -> FacilityResult<()>;
    fn get(&self, bias_name: &str) -> FacilityResult<i32>;
    fn get_all_biases(&self) -> FacilityResult<HashMap<String, i32>>;
}

pub trait MonitoringFacility {
    fn get_temperature(&self) -> FacilityResult<i32>;
    fn get_illumination(&self) -> FacilityResult<i32>;
}

pub trait PluginSoftwareInfoFacility {
    fn get_plugin_name(&self) -> String;
    fn get_version(&self) -> String;
}

pub trait ROIFacility {
    property! {
        enabled: bool;
    }

    fn set_roi(&mut self, region: Region) -> FacilityResult<()>;
    fn set_rois(&mut self, regions: &[Region]) -> FacilityResult<()>;
}

pub trait ROIPixelMaskFacility {
    property! {
        pixel_masks: Vec<PixelMask>;
    }
}

pub trait TriggerInFacility {
    fn enable(&mut self, channel: u32) -> FacilityResult<()>;
    fn disable(&mut self, channel: u32) -> FacilityResult<()>;
}

pub trait TriggerOutFacility {
    fn enable(&mut self) -> FacilityResult<()>;
    fn disable(&mut self) -> FacilityResult<()>;
    fn set_period(&mut self, period_us: u32) -> FacilityResult<()>;
    fn set_duty_cycle(&mut self, duty_cycle: f64) -> FacilityResult<()>;
}
