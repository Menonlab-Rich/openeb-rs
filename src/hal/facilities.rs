use crate::hal::errors::{DecoderProtocolViolation, HalError, HalResult};
use crate::hal::types::{Cb, CbRo, Event, EventSlice, PixelMask, Region};
use crate::property;
use std::any::Any;
use std::collections;

pub trait Facility: Any + Send + Sync {
    fn registration_info(&self) -> collections::hash_set::HashSet<usize>;
    fn as_any(&self) -> &dyn Any;
}

pub trait RegisterableFacility: Facility {
    fn class_registration_info(&self) -> usize;
}

#[derive(Debug)]
pub enum AntiFlickerMode {
    BandPass,
    BandStop,
}

#[derive(Debug)]
pub enum CameraSyncMode {
    StandAlone,
    Master,
    Slave,
}

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
    fn add_protocol_violation_callback(
        &mut self,
        cb: Cb<DecoderProtocolViolation>,
    ) -> HalResult<usize>;
    fn remove_protocol_violation_callback(&mut self, cb_id: usize) -> HalResult<()>; // consume the ID because
    // once its removed, you
    // shouldn't use it again.

    property! {
        raw_event_size_bytes: u8;
    }
}

pub trait DecoderFacility<T>: BaseDecoderFacility {
    fn decode(&mut self, raw_data: &[u8]) -> HalResult<()>;
    fn add_decode_callback(&mut self, cb: Cb<&[T]>) -> HalResult<usize>;
    fn remove_decode_callback(&mut self, cb_id: usize) -> HalResult<()>; // consume the ID because once
    // removed, you shouldn't use it
    // again.
}
pub trait BufferDecoderFacility<T>: BaseDecoderFacility {
    fn decode_to_buffer(&mut self, raw_data: &[u8], output: &mut Vec<T>) -> HalResult<()>;
}
pub trait CameraSyncFacility {
    property! {
        mode: CameraSyncMode;
    }
}

pub trait DigitalCropFacility {
    property! {
        enable: bool;
        window_region: Region;
    }
}
pub trait DigitalEventMaskFacility {
    property! {
        masks: &Vec<PixelMask>;
    }
}
pub trait ERCModuleFacility {
    property! {
        enable: bool;
        cd_event_rate: u32;
        max_supported_cd_event_rate: u32;
        min_supported_cd_event_rate: u32;
        count_period: u32;
        cd_event_count: u32;
        min_supported_cd_event_count: u32;
        max_supported_cd_event_count: u32;
    }

    fn erc_from_file(path: &String);
}
pub trait EventDecoderFacility {
    fn add_event_buffer_callback(callback: CbRo<EventSlice>) -> HalResult<usize>;
    fn remove_event_buffer_callback(id: usize) -> HalResult<()>;
    fn add_event_buffer(range: EventSlice);
}
pub trait EventFrameDecoderFacility {
    type FrameType;
    property! {
        width: u32;
        height: u32;
    }

    fn add_event_frame_cb(callback: CbRo<&Self::FrameType>) -> HalResult<usize>;
}
pub trait EventRateActivityFilterModuleFacility {}
pub trait EventsStreamFacility {}
pub trait GeometryFacility {}
pub trait HALSoftwareInfoFacility {}
pub trait HWIdentificationFacility {}
pub trait HWRegisterFacility {}
pub trait LLBiasesFacility {}
pub trait MonitoringFacility {}
pub trait PluginSoftwareInfoFacility {}
pub trait ROIFacility {}
pub trait ROIPixelMaskFacility {}
pub trait TriggerInFacility {}
pub trait TriggerOutFacility {}

impl Facility for AntiFlickerFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for CameraSyncFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for DecoderFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for DigitalCropFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for DigitalEventMaskFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for ERCModuleFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for EventDecoderFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for EventFrameDecoderFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for EventRateActivityFilterModuleFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for EventsStreamFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for GeometryFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for HALSoftwareInfoFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for HWIdentificationFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for HWRegisterFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for LLBiasesFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for MonitoringFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for PluginSoftwareInfoFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for ROIFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for ROIPixelMaskFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for TriggerInFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}
impl Facility for TriggerOutFacility {
    fn registration_info(&self) -> std::collections::HashSet<usize> {
        todo!()
    }
}

// Begin Facilitylity
impl RegisterableFacility for HWIdentificationFacility {
    fn class_registration_info(&self) -> usize {
        todo!()
    }
}
impl RegisterableFacility for HWRegisterFacility {
    fn class_registration_info(&self) -> usize {
        todo!()
    }
}
impl RegisterableFacility for LLBiasesFacility {
    fn class_registration_info(&self) -> usize {
        todo!()
    }
}
impl RegisterableFacility for MonitoringFacility {
    fn class_registration_info(&self) -> usize {
        todo!()
    }
}
impl RegisterableFacility for PluginSoftwareInfoFacility {
    fn class_registration_info(&self) -> usize {
        todo!()
    }
}
impl RegisterableFacility for ROIFacility {
    fn class_registration_info(&self) -> usize {
        todo!()
    }
}
impl RegisterableFacility for ROIPixelMaskFacility {
    fn class_registration_info(&self) -> usize {
        todo!()
    }
}
impl RegisterableFacility for TriggerInFacility {
    fn class_registration_info(&self) -> usize {
        todo!()
    }
}
impl RegisterableFacility for TriggerOutFacility {
    fn class_registration_info(&self) -> usize {
        todo!()
    }
}
