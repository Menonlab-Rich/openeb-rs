//! Facility abstractions for the HAL.
//!
//! A "facility" is a capability exposed by a device: geometry, hardware
//! identification, streams, decoders, ROI control, trigger control, and so on.
//! A device is represented as a registry rather than a single trait object. It is a
//! registry of individually typed capability objects.
//!
//! The pieces fit together like this:
//!
//! - `FacilityType` names the capability the caller wants.
//! - `FacilityHandle` stores the concrete trait object behind that capability.
//! - `Device::get_facility` returns the handle.
//! - `TryFrom<FacilityHandle>` converts the handle into the exact trait-object
//!   alias expected by the caller.
//! - `BaseFacility` provides `Any`-based downcasting when a mutable handle must
//!   be recovered as a concrete type.
//!
//! This design models heterogeneous hardware while keeping call sites explicit
//! about ownership, mutability, and thread-safety.

use crate::hal::errors::{
    DecoderError, DecoderProtocolViolation, HardwareError, ProcessingError, SharedError,
    StreamError,
};
use crate::hal::types::{Cb, CbRo, PixelMask, Region};
use crate::hal::types::{EventCD, EventExtTrigger};
use crossbeam::channel::Receiver;
pub use macros::pack_facility;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use thiserror::Error;
use utilities::buffer::PooledBuffer;

/// Type alias for the immutable geometry facility handle.
pub type GeometryFacilityHandle = Arc<dyn GeometryFacility + Send + Sync>;
pub type HALSoftwareInfoFacilityHandle = Arc<dyn HALSoftwareInfoFacility + Send + Sync>;
pub type HWIdentificationFacilityHandle = Arc<dyn HWIdentificationFacility + Send + Sync>;
pub type MonitoringFacilityHandle = Arc<dyn MonitoringFacility + Send + Sync>;
pub type PluginSoftwareInfoFacilityHandle = Arc<dyn PluginSoftwareInfoFacility + Send + Sync>;

/// Type alias for a mutable facility handle wrapped in `Arc<RwLock<_>>`.
///
/// Mutable facilities are shared between consumers, but the trait object itself
/// must be locked before use because its methods can mutate internal state.
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

use std::any::Any;
use std::convert::TryFrom;

/// Error returned when a caller requests a facility handle of the wrong type.
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

/// Unified error type for failures that occur while working through a facility.
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
    #[error("Invalid downcast from {0} to {1}")]
    FacilityDowncastError(String, String),
}

/// Result type used by facility methods.
pub type FacilityResult<T> = Result<T, FacilityError>;

/// Identifies a capability a device may expose.
///
/// This enum is the lookup key used with `Device::get_facility`. It is broader
/// than any single device implementation so code can ask for a capability
/// without knowing the concrete type up front.
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
    EventDecoderFacility,
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

/// Stores the concrete trait object for a facility.
///
/// Immutable facilities are stored as plain `Arc<T>`. Mutable facilities are
/// stored as `Arc<RwLock<T>>` so callers can share them while still taking
/// exclusive write access when they need to mutate device state.
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

/// Base trait shared by all facilities.
///
/// The `Any` hooks support dynamic downcasting when a device stores multiple
/// different facility trait objects in one registry.
pub trait BaseFacility: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Any + Sized + Send + Sync> BaseFacility for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// --- Facilities ---
/// Controls anti-flicker behavior for the device.
///
/// The exact meaning of the thresholds and frequency fields is device-specific
pub trait AntiFlickerFacility: BaseFacility {
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

/// Base interface for decoders that emit protocol violations and expose the raw
/// event word size.
pub trait BaseDecoderFacility: BaseFacility {
    fn subscribe_to_protocol_violation(&mut self) -> Receiver<SharedError>;

    property! {
        ro raw_event_size_bytes: u8;
    }
}

/// Decodes raw byte streams into typed events and decode callbacks.
///
/// Implementations are expected to consume the input buffer in order and invoke
/// registered callbacks as events are decoded.
pub trait DecoderFacility<T>: BaseDecoderFacility {
    fn decode(&mut self, raw_data: &[u8]) -> FacilityResult<()>;
    fn add_decode_callback(&mut self, cb: Cb<&[T]>) -> FacilityResult<usize>;

    // Consume the ID because it is no longer registered after removal.
    fn remove_decode_callback(&mut self, cb_id: usize) -> FacilityResult<()>;
}

/// Decodes raw data directly into a caller-provided buffer.
///
/// TODO: clarify whether this trait is intended to be a lower-level alternative
/// to `DecoderFacility` or a separate implementation strategy.
pub trait BufferDecoderFacility<T>: BaseDecoderFacility {
    fn decode_to_buffer(&mut self, raw_data: &[u8], output: &mut Vec<T>) -> FacilityResult<()>;
}

/// Controls the camera synchronization mode.
pub trait CameraSyncFacility: BaseFacility {
    property! {
        mode: CameraSyncMode;
    }
}

/// Enables or configures a sensor crop window.
pub trait DigitalCropFacility: BaseFacility {
    property! {
        enabled: bool;
        window_region: Region;
    }
}

/// Controls per-pixel event masking.
pub trait DigitalEventMaskFacility: BaseFacility {
    property! {
        masks: Vec<PixelMask>;
    }
}

/// Configures event-rate control logic.
pub trait ERCModuleFacility: BaseFacility {
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

/// Receives decoded CD and external-trigger event batches from a decoder.
pub trait EventDecoderFacility: BaseFacility {
    fn subscribe_to_cd_events(&mut self) -> Receiver<Arc<PooledBuffer<EventCD>>>;
    fn add_event_buffer(&mut self, range: Arc<PooledBuffer<EventCD>>);
    fn subscribe_to_ext_events(&mut self) -> Receiver<Arc<PooledBuffer<EventExtTrigger>>>;
}

/// Decodes events into a framed output type.
///
/// TODO: document the frame callback lifecycle and what the associated
/// `FrameType` is expected to represent for RGB vs mono decoders.
pub trait EventFrameDecoderFacility: BaseFacility {
    type FrameType;
    property! {
        width: u32;
        height: u32;
    }

    fn add_event_frame_cb(&mut self, callback: CbRo<&Self::FrameType>) -> FacilityResult<usize>;
}

/// Controls the event trail filter module.
pub trait EventTrailFilterModuleFacility: BaseFacility {
    property! {
        enabled: bool;
        filter_type: TrailFilterTypes;
        threshold: u32;
    }
    fn get_available_types(&self) -> &HashSet<TrailFilterTypes>;
    fn get_max_supported_threshold(&self) -> u32;
    fn get_min_supported_threshold(&self) -> u32;
}

/// Controls the event-rate activity filter module.
pub trait EventRateActivityFilterModuleFacility: BaseFacility {
    property! {
        enabled: bool;
        thresholds: (u32, u32);
    }
}

/// Abstracts the raw byte stream coming from a file or device.
pub trait EventsStreamFacility: BaseFacility {
    fn start(&mut self) -> FacilityResult<()>;
    fn stop(&mut self) -> FacilityResult<()>;
    fn poll_buffer(&mut self) -> FacilityResult<(&[u8], usize)>;
    fn wait_next_buffer(&mut self) -> FacilityResult<(&[u8], usize)>;
}

/// Decodes raw stream buffers into typed events.
///
/// This is the main decoder contract used by file-backed readers.
/// It maintains internal timestamp state and dispatches decoded batches
/// through the event facilities.
pub trait EventsStreamDecoderFacility: BaseDecoderFacility + BaseFacility {
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

/// Exposes the sensor geometry.
pub trait GeometryFacility: BaseFacility {
    fn get_width(&self) -> i32;
    fn get_height(&self) -> i32;
}

/// Exposes software version information for the HAL implementation.
///
/// TODO: clarify whether this represents the camera firmware version, the
/// host-side HAL version, or both.
pub trait HALSoftwareInfoFacility: BaseFacility {
    fn get_version(&self) -> String;
}

/// Exposes hardware identity and file/sensor metadata.
pub trait HWIdentificationFacility: BaseFacility {
    fn get_serial(&self) -> FacilityResult<String>;
    fn get_system_id(&self) -> FacilityResult<i64>;
    fn get_sensor_info(&self) -> FacilityResult<SensorInfo>;
    fn get_system_info(&self) -> FacilityResult<SystemInfo>;
    fn get_connection_type(&self) -> FacilityResult<ConnectionType>;
    fn get_available_data_encoding_formats(&self) -> FacilityResult<Vec<String>>;
    fn get_current_data_encoding_format(&self) -> FacilityResult<String>;
}

/// Reads and writes device registers.
pub trait HWRegisterFacility: BaseFacility {
    fn read_register(&self, address: u32) -> FacilityResult<u32>;
    fn write_register(&mut self, address: u32, value: u32) -> FacilityResult<()>;
}

/// Reads and writes low-level bias settings.
pub trait LLBiasesFacility: BaseFacility {
    fn set(&mut self, bias_name: &str, bias_value: i32) -> FacilityResult<()>;
    fn get(&self, bias_name: &str) -> FacilityResult<i32>;
    fn get_all_biases(&self) -> FacilityResult<HashMap<String, i32>>;
}

/// Exposes monitoring information such as temperature and illumination.
pub trait MonitoringFacility: BaseFacility {
    fn get_temperature(&self) -> FacilityResult<i32>;
    fn get_illumination(&self) -> FacilityResult<i32>;
}

/// Exposes plugin metadata.
pub trait PluginSoftwareInfoFacility: BaseFacility {
    fn get_plugin_name(&self) -> String;
    fn get_version(&self) -> String;
}

/// Controls region-of-interest settings.
pub trait ROIFacility: BaseFacility {
    property! {
        enabled: bool;
    }

    fn set_roi(&mut self, region: Region) -> FacilityResult<()>;
    fn set_rois(&mut self, regions: &[Region]) -> FacilityResult<()>;
    fn roi(&self) -> Option<Region>;
    fn rois(&self) -> Option<&[Region]>;
}

/// Controls pixel masks as a batch of individual mask entries.
pub trait ROIPixelMaskFacility: BaseFacility {
    property! {
        pixel_masks: Vec<PixelMask>;
    }
}

/// Controls external trigger input channels.
pub trait TriggerInFacility: BaseFacility {
    fn enable(&mut self, channel: u32) -> FacilityResult<()>;
    fn disable(&mut self, channel: u32) -> FacilityResult<()>;
}

/// Controls external trigger output timing.
pub trait TriggerOutFacility: BaseFacility {
    fn enable(&mut self) -> FacilityResult<()>;
    fn disable(&mut self) -> FacilityResult<()>;
    fn set_period(&mut self, period_us: u32) -> FacilityResult<()>;
    fn set_duty_cycle(&mut self, duty_cycle: f64) -> FacilityResult<()>;
}
