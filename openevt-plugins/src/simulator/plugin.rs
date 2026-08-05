use crate::simulator::error::SimError;
use crate::simulator::solver::EvsParameters;
use ffmpeg_next::codec;
use ffmpeg_next::format::{Pixel, input as ffmpeg_input};
use ffmpeg_next::software::scaling::{context::Context, flag::Flags};
use openevt::hal::device::plugin;
use std::{error::Error, fs};

use abi_stable::std_types::{RBox, ROption, RResult, RSlice, RStr, RString, RVec};

pub struct SimulatorDevice {}

impl plugin::DevicePlugin for SimulatorDevice {
    #[allow(clippy::let_and_return)]
    fn serial(&self) -> RString where {
        let serial = "EventSimulator";
        serial.into()
    }

    #[allow(clippy::let_and_return)]
    fn connection_type(&self) -> ConnectionType where {
        todo!()
    }

    #[allow(clippy::let_and_return)]
    fn geometry(&self) -> PluginGeometry where {
        todo!()
    }

    #[allow(clippy::let_and_return)]
    fn t_min(&self) -> ROption<usize> where {
        todo!()
    }

    #[allow(clippy::let_and_return)]
    fn t_max(&self) -> ROption<usize> where {
        todo!()
    }

    #[allow(clippy::let_and_return)]
    fn seek(&mut self, timestamp: u32) -> RResult<(), RString> where {
        todo!()
    }

    #[allow(clippy::let_and_return)]
    fn seek_to_next_ext(&mut self) -> RResult<(), RString> where {
        todo!()
    }

    #[allow(clippy::let_and_return)]
    fn get_facilities(&self) -> RVec<PluginFacilityType> where {
        todo!()
    }

    #[allow(clippy::let_and_return)]
    fn get_facility(&self, facility_type: PluginFacilityType) -> ROption<PluginFacility> where {
        todo!()
    }

    #[allow(clippy::let_and_return)]
    fn start_events(&mut self, sink: EventBatchSinkBox) -> RResult<(), RString> where {
        todo!()
    }

    #[allow(clippy::let_and_return)]
    fn start_external_triggers(&mut self, sink: EventBatchSinkBox) -> RResult<(), RString> where {
        todo!()
    }

    #[allow(clippy::let_and_return)]
    fn load_batch(&mut self) -> RResult<(), RString> where {
        todo!()
    }
}

fn from_toml(path: &str) -> Result<EvsParameters, SimError> {
    let toml_str = fs::read_to_string(path)?;
    toml::from_str(&toml_str)?
}

fn parse_frames(path: &str) -> Result<(), ffmpeg_next::Error> {
    ffmpeg_next::init()?;
    let mut ictx = ffmpeg_input(path)?;
    let input = ictx
        .streams()
        .best(ffmpeg_next::media::Type::Video)
        .ok_or(ffmpeg_next::Error::StreamNotFound)?;
    let stream_idx = input.index();
    let context_decoder = codec::context::Context::from_parameters(input.parameters())?;
    let mut decoder = context_decoder.decoder().video()?;

    let mut scaler = Context::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        Pixel::RGB24,
        decoder.width(),
        decoder.height(),
        Flags::BILINEAR,
    )?;

    Ok(())
}
