//! Stable ABI contract for third-party device plugins.
//!
//! This module is intentionally separate from the native HAL traits. Native
//! facilities currently expose Rust-only types (crossbeam receivers, borrowed
//! slices and `Any` downcasts); putting those types in a shared-library ABI
//! would make the ABI unsound. Plugins use these stable descriptors and opaque
//! facility handles, while hosts may add richer adapters over time.

use abi_stable::{
    StableAbi,
    library::RootModule,
    package_version_strings, sabi_trait,
    sabi_types::VersionStrings,
    std_types::{RBox, ROption, RResult, RSlice, RStr, RString, RVec},
};

use super::discovery::ConnectionType;
use crate::hal::facilities::FacilityType;
use crate::hal::types::{EventCD, EventExtTrigger};

/// ABI representation of [`PluginCameraDescription`]. Keeping this separate
/// means the native HAL remains usable without enabling the plugin feature.
#[derive(Debug, Clone, PartialEq, Eq, StableAbi)]
#[repr(C)]
pub struct PluginCameraDescriptionAbi {
    pub serial: RString,
    pub connection: ConnectionType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
#[repr(C)]
pub struct PluginGeometry {
    pub width: u32,
    pub height: u32,
}

impl From<PluginCameraDescriptionAbi> for super::discovery::PluginCameraDescription {
    fn from(value: PluginCameraDescriptionAbi) -> Self {
        Self {
            serial: value.serial.into(),
            connection: value.connection,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
#[repr(C)]
pub enum PluginFacilityType {
    AntiFlicker,
    BaseDecoder,
    CameraSync,
    Decoder,
    DigitalCrop,
    DigitalEventMask,
    ERCModule,
    CDDecoder,
    TriggerEventDecoder,
    ERCCounterDecoder,
    RGBEventFrameDecoder,
    MonoEventFrameDecoder,
    EventRateActivityFilterModule,
    EventTrailFilterModule,
    HWRegister,
    LLBiases,
    HALSoftwareInfo,
    PluginSoftwareInfo,
    Monitoring,
    ROIPixelMask,
    TriggerIn,
    TriggerOut,
    Geometry,
    HardwareIdentification,
    EventsStream,
    EventsStreamDecoder,
    EventDecoder,
    Roi,
    Other,
}

impl PluginFacilityType {
    /// Every facility key understood by the plugin ABI.
    ///
    /// `Other` is deliberately excluded: it is the forward-compatible escape
    /// hatch for a plugin-specific, versioned facility and is not a native HAL
    /// facility.
    pub const ALL: [Self; 28] = [
        Self::AntiFlicker,
        Self::BaseDecoder,
        Self::CameraSync,
        Self::Decoder,
        Self::DigitalCrop,
        Self::DigitalEventMask,
        Self::ERCModule,
        Self::CDDecoder,
        Self::TriggerEventDecoder,
        Self::ERCCounterDecoder,
        Self::RGBEventFrameDecoder,
        Self::MonoEventFrameDecoder,
        Self::EventRateActivityFilterModule,
        Self::EventTrailFilterModule,
        Self::EventsStream,
        Self::EventsStreamDecoder,
        Self::EventDecoder,
        Self::Geometry,
        Self::HALSoftwareInfo,
        Self::HardwareIdentification,
        Self::HWRegister,
        Self::LLBiases,
        Self::Monitoring,
        Self::PluginSoftwareInfo,
        Self::Roi,
        Self::ROIPixelMask,
        Self::TriggerIn,
        Self::TriggerOut,
    ];

    pub const fn is_native(self) -> bool {
        !matches!(self, Self::Other)
    }
}

impl From<FacilityType> for PluginFacilityType {
    fn from(value: FacilityType) -> Self {
        match value {
            FacilityType::AntiFlickerFacility => Self::AntiFlicker,
            FacilityType::BaseDecoderFacility => Self::BaseDecoder,
            FacilityType::CameraSyncFacility => Self::CameraSync,
            FacilityType::DecoderFacility => Self::Decoder,
            FacilityType::DigitalCropFacility => Self::DigitalCrop,
            FacilityType::DigitalEventMaskFacility => Self::DigitalEventMask,
            FacilityType::ERCModuleFacility => Self::ERCModule,
            FacilityType::CDDecoderFacility => Self::CDDecoder,
            FacilityType::TriggerEventDecoderFaciliy => Self::TriggerEventDecoder,
            FacilityType::ERCCounterDecoderFacility => Self::ERCCounterDecoder,
            FacilityType::RGBEventFrameDecoderFacility => Self::RGBEventFrameDecoder,
            FacilityType::MonoEventFrameDecoderFacility => Self::MonoEventFrameDecoder,
            FacilityType::EventRateActivityFilterModuleFacility => {
                Self::EventRateActivityFilterModule
            }
            FacilityType::EventTrailFilterModuleFacility => Self::EventTrailFilterModule,
            FacilityType::EventsStreamFacility => Self::EventsStream,
            FacilityType::EventsStreamDecoderFacility => Self::EventsStreamDecoder,
            FacilityType::EventDecoderFacility => Self::EventDecoder,
            FacilityType::GeometryFacility => Self::Geometry,
            FacilityType::HALSoftwareInfoFacility => Self::HALSoftwareInfo,
            FacilityType::HWIdentificationFacility => Self::HardwareIdentification,
            FacilityType::HWRegisterFacility => Self::HWRegister,
            FacilityType::LLBiasesFacility => Self::LLBiases,
            FacilityType::MonitoringFacility => Self::Monitoring,
            FacilityType::PluginSoftwareInfoFacility => Self::PluginSoftwareInfo,
            FacilityType::ROIFacility => Self::Roi,
            FacilityType::ROIPixelMaskFacility => Self::ROIPixelMask,
            FacilityType::TriggerInFacility => Self::TriggerIn,
            FacilityType::TriggerOutFacility => Self::TriggerOut,
        }
    }
}

/// FFI-safe opaque facility descriptor. Concrete operations can be added as
/// new versioned traits without changing the discovery ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
#[repr(C)]
pub struct PluginFacility {
    pub facility_type: PluginFacilityType,
}

impl PluginFacility {
    pub const fn new(facility_type: PluginFacilityType) -> Self {
        Self { facility_type }
    }
}

/// Callback sink used instead of exposing crossbeam channels across the ABI.
/// Each call represents one decoded batch.
#[sabi_trait]
pub trait EventBatchSink: Send + Sync {
    fn on_cd_events(&self, events: RSlice<'_, EventCD>);
    fn on_ext_events(&self, events: RSlice<'_, EventExtTrigger>);
}

pub type EventBatchSinkBox = EventBatchSink_TO<'static, RBox<()>>;

#[sabi_trait]
pub trait DevicePlugin: Send + Sync {
    fn serial(&self) -> RString;
    fn connection_type(&self) -> ConnectionType;
    fn geometry(&self) -> PluginGeometry;
    fn t_min(&self) -> ROption<usize>;
    fn t_max(&self) -> ROption<usize>;
    fn seek(&mut self, timestamp: u32) -> RResult<(), RString>;
    fn seek_to_next_ext(&mut self) -> RResult<(), RString>;
    fn get_facilities(&self) -> RVec<PluginFacilityType>;
    fn get_facility(&self, facility_type: PluginFacilityType) -> ROption<PluginFacility>;
    fn start_events(&mut self, sink: EventBatchSinkBox) -> RResult<(), RString>;
    fn start_external_triggers(&mut self, sink: EventBatchSinkBox) -> RResult<(), RString>;
    fn load_batch(&mut self) -> RResult<(), RString>;
}

pub type DevicePluginBox = DevicePlugin_TO<'static, RBox<()>>;

#[sabi_trait]
pub trait DeviceDiscoveryPlugin: Send + Sync {
    fn discover(&self) -> RVec<PluginCameraDescriptionAbi>;
    fn open_device(&self, serial: RStr<'_>) -> RResult<DevicePluginBox, RString>;
}

pub type DeviceDiscoveryPluginBox = DeviceDiscoveryPlugin_TO<'static, RBox<()>>;

#[derive(StableAbi)]
#[repr(C)]
#[sabi(kind(Prefix(prefix_ref = DevicePluginModuleRef)))]
pub struct DevicePluginModuleVtable {
    pub name: extern "C" fn() -> RString,
    pub create_discovery: extern "C" fn() -> DeviceDiscoveryPluginBox,
}

impl RootModule for DevicePluginModuleRef {
    abi_stable::declare_root_module_statics! {DevicePluginModuleRef}
    const BASE_NAME: &'static str = "openevt_device_plugin";
    const NAME: &'static str = "openevt_device_plugin";
    const VERSION_STRINGS: VersionStrings = package_version_strings!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_covers_every_native_facility() {
        assert_eq!(PluginFacilityType::ALL.len(), 28);
        assert!(PluginFacilityType::ALL.iter().all(|kind| kind.is_native()));

        let native = [
            FacilityType::AntiFlickerFacility,
            FacilityType::BaseDecoderFacility,
            FacilityType::CameraSyncFacility,
            FacilityType::DecoderFacility,
            FacilityType::DigitalCropFacility,
            FacilityType::DigitalEventMaskFacility,
            FacilityType::ERCModuleFacility,
            FacilityType::CDDecoderFacility,
            FacilityType::TriggerEventDecoderFaciliy,
            FacilityType::ERCCounterDecoderFacility,
            FacilityType::RGBEventFrameDecoderFacility,
            FacilityType::MonoEventFrameDecoderFacility,
            FacilityType::EventRateActivityFilterModuleFacility,
            FacilityType::EventTrailFilterModuleFacility,
            FacilityType::EventsStreamFacility,
            FacilityType::EventsStreamDecoderFacility,
            FacilityType::EventDecoderFacility,
            FacilityType::GeometryFacility,
            FacilityType::HALSoftwareInfoFacility,
            FacilityType::HWIdentificationFacility,
            FacilityType::HWRegisterFacility,
            FacilityType::LLBiasesFacility,
            FacilityType::MonitoringFacility,
            FacilityType::PluginSoftwareInfoFacility,
            FacilityType::ROIFacility,
            FacilityType::ROIPixelMaskFacility,
            FacilityType::TriggerInFacility,
            FacilityType::TriggerOutFacility,
        ];

        assert!(
            native
                .into_iter()
                .map(PluginFacilityType::from)
                .all(|kind| PluginFacilityType::ALL.contains(&kind))
        );
    }

    #[test]
    fn opaque_facility_descriptor_preserves_kind() {
        let descriptor = PluginFacility::new(PluginFacilityType::Roi);
        assert_eq!(descriptor.facility_type, PluginFacilityType::Roi);
    }
}
