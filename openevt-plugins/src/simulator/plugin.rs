//! Video-backed event-camera simulator plugin.

use crate::simulator::error::SimError;
use crate::simulator::solver::{EvsParameters, EvsSimulator};
use abi_stable::prefix_type::PrefixTypeTrait;
use abi_stable::std_types::{ROption, RResult, RString, RVec};
use abi_stable::type_level::downcasting::TD_Opaque;
use ffmpeg_next::codec;
use ffmpeg_next::format::{self, Pixel, input as ffmpeg_input};
use ffmpeg_next::software::scaling::{context::Context, flag::Flags};
use ffmpeg_next::util::error::{EAGAIN, Error as FfmpegError};
use ffmpeg_next::util::frame::video::Video;
use openevt::hal::device::configuration::PluginConfigurationSchema;
use openevt::hal::device::discovery::ConnectionType;
use openevt::hal::device::plugin::{
    self, DeviceDiscoveryPlugin, DeviceDiscoveryPlugin_TO, DeviceDiscoveryPluginBox, DevicePlugin,
    DevicePlugin_TO, DevicePluginBox, DevicePluginModuleRef, DevicePluginModuleVtable,
    EventBatchSinkBox, PluginCameraDescriptionAbi, PluginConfiguration,
    PluginEventSubscriptionFacility, PluginEventSubscriptionFacility_TO, PluginFacility,
    PluginFacilityHandle, PluginFacilityType, PluginGeometry, PluginGeometryFacility,
    PluginGeometryFacility_TO, PluginRawEventStreamFacility, PluginRawEventStreamFacility_TO,
    PluginStreamBuffer,
};
use openevt::types::EventCD;
use std::fs;
use std::sync::{Arc, Mutex};

const SIMULATOR_SERIAL: &str = "EventSimulator";
const RAW_EVENT_SIMULATOR_SCHEMA: &str = r#"
version = 1

[[parameters]]
name = "video_file"
label = "Video file"
kind = "file"
required = true
description = "A video file whose frames will be converted into simulated events."

[[parameters]]
name = "fps"
label = "Frames per second"
kind = "float"
required = false
description = "Optional override for the video's encoded frame rate."

[[parameters]]
name = "config_file"
label = "Simulator configuration"
kind = "file"
required = false
description = "Optional TOML file containing the initial simulator parameters."
"#;

struct SimulatorGeometryFacility {
    width: u32,
    height: u32,
}

struct VideoSimulator {
    input: format::context::Input,
    decoder: codec::decoder::Video,
    scaler: Context,
    video_stream_index: usize,
    width: usize,
    height: usize,
    fps: f64,
    frame_index: u64,
    packet_pending: bool,
    eof_sent: bool,
    simulator: EvsSimulator,
}

// FFmpeg's Rust wrapper does not mark the scaler context as `Send` because it
// contains an opaque C pointer. The entire simulator is accessed through one
// mutex, so the context is never used concurrently or moved independently of
// the other decoder state.
unsafe impl Send for VideoSimulator {}

impl VideoSimulator {
    fn open(
        video_path: &str,
        fps_override: Option<f64>,
        params: EvsParameters,
    ) -> Result<Self, String> {
        ffmpeg_next::init().map_err(|error| error.to_string())?;
        params.validate().map_err(|error| error.to_string())?;

        let input = ffmpeg_input(video_path).map_err(|error| error.to_string())?;
        let stream = input
            .streams()
            .best(ffmpeg_next::media::Type::Video)
            .ok_or_else(|| "video file does not contain a video stream".to_owned())?;
        let video_stream_index = stream.index();
        let encoded_fps = f64::from(stream.avg_frame_rate());
        let fps = fps_override.unwrap_or(encoded_fps);
        if !fps.is_finite() || fps <= 0.0 {
            return Err("video frame rate must be finite and positive".to_owned());
        }

        let context_decoder = codec::context::Context::from_parameters(stream.parameters())
            .map_err(|error| error.to_string())?;
        let decoder = context_decoder
            .decoder()
            .video()
            .map_err(|error| error.to_string())?;
        let width = decoder.width() as usize;
        let height = decoder.height() as usize;
        if width == 0 || height == 0 {
            return Err("video stream has zero dimensions".to_owned());
        }
        let scaler = Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            Pixel::RGB24,
            decoder.width(),
            decoder.height(),
            Flags::BILINEAR,
        )
        .map_err(|error| error.to_string())?;

        Ok(Self {
            input,
            decoder,
            scaler,
            video_stream_index,
            width,
            height,
            fps,
            frame_index: 0,
            packet_pending: false,
            eof_sent: false,
            simulator: EvsSimulator::new(width * height, params)
                .map_err(|error| error.to_string())?,
        })
    }

    fn next_events(&mut self) -> Result<Option<Vec<EventCD>>, String> {
        loop {
            if self.packet_pending {
                let mut decoded = Video::empty();
                match self.decoder.receive_frame(&mut decoded) {
                    Ok(()) => return self.process_frame(&decoded).map(Some),
                    Err(FfmpegError::Other { errno }) if errno == EAGAIN => {
                        self.packet_pending = false;
                    }
                    Err(FfmpegError::Eof) => return Ok(None),
                    Err(error) => return Err(error.to_string()),
                }
            }

            if self.eof_sent {
                return Ok(None);
            }

            let next_packet = self.input.packets().next();
            let Some((stream, packet)) = next_packet else {
                self.decoder.send_eof().map_err(|error| error.to_string())?;
                self.eof_sent = true;
                self.packet_pending = true;
                continue;
            };
            if stream.index() != self.video_stream_index {
                continue;
            }
            self.decoder
                .send_packet(&packet)
                .map_err(|error| error.to_string())?;
            self.packet_pending = true;
        }
    }

    fn process_frame(&mut self, decoded: &Video) -> Result<Vec<EventCD>, String> {
        let mut rgb = Video::empty();
        self.scaler
            .run(decoded, &mut rgb)
            .map_err(|error| error.to_string())?;

        let stride = rgb.stride(0);
        let data = rgb.data(0);
        let mut photocurrents = Vec::with_capacity(self.width * self.height);
        for y in 0..self.height {
            let row = &data[y * stride..];
            for x in 0..self.width {
                let offset = x * 3;
                let red = row[offset] as f32;
                let green = row[offset + 1] as f32;
                let blue = row[offset + 2] as f32;
                let luminance = (0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255.0;
                // Keep a dark pixel above zero while preserving the video's
                // relative brightness as the simulated photocurrent.
                photocurrents.push(luminance);
            }
        }

        let timestamp = (self.frame_index as f64 * 1_000_000.0 / self.fps) as f32;
        self.frame_index += 1;
        let generated = self
            .simulator
            .process_frame(&photocurrents, timestamp)
            .map_err(|error| error.to_string())?;

        Ok(generated
            .into_iter()
            .map(|event| EventCD {
                x: event.pixel_index % self.width,
                y: event.pixel_index / self.width,
                p: event.polarity,
                t: event.timestamp.max(0.0) as usize,
            })
            .collect())
    }
}

struct SimulatorState {
    started: bool,
    simulator: VideoSimulator,
    cd_sinks: Vec<EventBatchSinkBox>,
    ext_sinks: Vec<EventBatchSinkBox>,
}

impl SimulatorState {
    fn advance(state: &Arc<Mutex<Self>>) -> Result<(), String> {
        let (events, mut cd_sinks, mut ext_sinks) = {
            let mut lock = state
                .lock()
                .map_err(|_| "simulator state lock was poisoned".to_owned())?;
            if !lock.started {
                return Err("simulator stream has not been started".to_owned());
            }
            let events = lock
                .simulator
                .next_events()?
                .ok_or_else(|| "end of simulator video".to_owned())?;
            (
                events,
                std::mem::take(&mut lock.cd_sinks),
                std::mem::take(&mut lock.ext_sinks),
            )
        };

        for sink in &cd_sinks {
            sink.on_cd_events(events.as_slice().into());
        }
        for sink in &ext_sinks {
            sink.on_ext_events([].as_slice().into());
        }

        let mut lock = state
            .lock()
            .map_err(|_| "simulator state lock was poisoned".to_owned())?;
        lock.cd_sinks.append(&mut cd_sinks);
        lock.ext_sinks.append(&mut ext_sinks);
        Ok(())
    }
}

struct SimulatorDevice {
    state: Arc<Mutex<SimulatorState>>,
    width: u32,
    height: u32,
}

struct SimulatorStreamFacility {
    state: Arc<Mutex<SimulatorState>>,
}

impl SimulatorStreamFacility {
    fn request(&mut self) -> RResult<PluginStreamBuffer, RString> {
        match SimulatorState::advance(&self.state) {
            Ok(()) => RResult::ROk(PluginStreamBuffer {
                data: Vec::new().into(),
                valid_len: 0,
            }),
            Err(error) => RResult::RErr(error.into()),
        }
    }
}

impl PluginRawEventStreamFacility for SimulatorStreamFacility {
    fn start(&mut self) -> RResult<(), RString> {
        match self.state.lock() {
            Ok(mut state) => {
                state.started = true;
                RResult::ROk(())
            }
            Err(_) => RResult::RErr("simulator state lock was poisoned".into()),
        }
    }

    fn stop(&mut self) -> RResult<(), RString> {
        match self.state.lock() {
            Ok(mut state) => {
                state.started = false;
                RResult::ROk(())
            }
            Err(_) => RResult::RErr("simulator state lock was poisoned".into()),
        }
    }

    fn poll_buffer(&mut self) -> RResult<PluginStreamBuffer, RString> {
        self.request()
    }

    fn wait_next_buffer(&mut self) -> RResult<PluginStreamBuffer, RString> {
        self.request()
    }
}

struct SimulatorEventSubscription {
    state: Arc<Mutex<SimulatorState>>,
}

impl PluginEventSubscriptionFacility for SimulatorEventSubscription {
    fn subscribe_to_cd_events(&mut self, sink: EventBatchSinkBox) -> RResult<(), RString> {
        match self.state.lock() {
            Ok(mut state) => {
                state.cd_sinks.push(sink);
                RResult::ROk(())
            }
            Err(_) => RResult::RErr("simulator state lock was poisoned".into()),
        }
    }

    fn subscribe_to_ext_events(&mut self, sink: EventBatchSinkBox) -> RResult<(), RString> {
        match self.state.lock() {
            Ok(mut state) => {
                state.ext_sinks.push(sink);
                RResult::ROk(())
            }
            Err(_) => RResult::RErr("simulator state lock was poisoned".into()),
        }
    }
}

fn configuration_value<'a>(configuration: &'a PluginConfiguration, name: &str) -> Option<&'a str> {
    configuration
        .values
        .iter()
        .find(|value| value.name.as_str() == name)
        .and_then(|value| value.value.as_ref().into_option())
        .map(|value| value.as_str())
}

fn simulator_parameters(configuration: &PluginConfiguration) -> Result<EvsParameters, SimError> {
    let Some(path) = configuration_value(configuration, "config_file") else {
        let parameters = EvsParameters::default();
        parameters
            .validate()
            .map_err(SimError::InvalidConfiguration)?;
        return Ok(parameters);
    };
    let source = fs::read_to_string(path)?;
    let parameters = toml::from_str::<EvsParameters>(&source)?;
    parameters
        .validate()
        .map_err(SimError::InvalidConfiguration)?;
    Ok(parameters)
}

impl SimulatorDevice {
    fn open(configuration: &PluginConfiguration) -> Result<Self, SimError> {
        let video_path = configuration_value(configuration, "video_file")
            .ok_or_else(|| SimError::InvalidConfiguration("`video_file` is required".into()))?;
        let fps = configuration_value(configuration, "fps")
            .map(|value| {
                value
                    .parse::<f64>()
                    .map_err(|_| SimError::InvalidConfiguration("`fps` must be a number".into()))
            })
            .transpose()?;
        let parameters = simulator_parameters(configuration)?;
        let simulator = VideoSimulator::open(video_path, fps, parameters)
            .map_err(SimError::InvalidConfiguration)?;
        Ok(Self {
            width: simulator.width as u32,
            height: simulator.height as u32,
            state: Arc::new(Mutex::new(SimulatorState {
                started: false,
                simulator,
                cd_sinks: Vec::new(),
                ext_sinks: Vec::new(),
            })),
        })
    }
}

impl PluginGeometryFacility for SimulatorGeometryFacility {
    fn get_width(&self) -> u32 {
        self.width
    }
    fn get_height(&self) -> u32 {
        self.height
    }
}

impl plugin::DevicePlugin for SimulatorDevice {
    fn serial(&self) -> RString {
        SIMULATOR_SERIAL.into()
    }

    fn connection_type(&self) -> ConnectionType {
        ConnectionType::Software
    }

    fn geometry(&self) -> PluginGeometry {
        PluginGeometry {
            width: self.width,
            height: self.height,
        }
    }

    fn get_facilities(&self) -> RVec<PluginFacilityType> {
        vec![
            PluginFacilityType::Geometry,
            PluginFacilityType::RawEventStream,
            PluginFacilityType::EventSubscription,
        ]
        .into()
    }

    fn get_facility(&self, facility_type: PluginFacilityType) -> ROption<PluginFacility> {
        if self.get_facilities().contains(&facility_type) {
            Some(PluginFacility { facility_type }).into()
        } else {
            ROption::RNone
        }
    }

    fn get_facility_handle(
        &self,
        facility_type: PluginFacilityType,
    ) -> ROption<PluginFacilityHandle> {
        match facility_type {
            PluginFacilityType::Geometry => Some(PluginFacilityHandle::Geometry(
                PluginGeometryFacility_TO::from_value(
                    SimulatorGeometryFacility {
                        width: self.width,
                        height: self.height,
                    },
                    TD_Opaque,
                ),
            ))
            .into(),
            PluginFacilityType::RawEventStream => Some(PluginFacilityHandle::RawEventStream(
                PluginRawEventStreamFacility_TO::from_value(
                    SimulatorStreamFacility {
                        state: Arc::clone(&self.state),
                    },
                    TD_Opaque,
                ),
            ))
            .into(),
            PluginFacilityType::EventSubscription => Some(PluginFacilityHandle::EventSubscription(
                PluginEventSubscriptionFacility_TO::from_value(
                    SimulatorEventSubscription {
                        state: Arc::clone(&self.state),
                    },
                    TD_Opaque,
                ),
            ))
            .into(),
            _ => ROption::RNone,
        }
    }

    fn start_events(&mut self, sink: EventBatchSinkBox) -> RResult<(), RString> {
        match self.state.lock() {
            Ok(mut state) => {
                state.started = true;
                state.cd_sinks.push(sink);
                RResult::ROk(())
            }
            Err(_) => RResult::RErr("simulator state lock was poisoned".into()),
        }
    }

    fn start_external_triggers(&mut self, sink: EventBatchSinkBox) -> RResult<(), RString> {
        match self.state.lock() {
            Ok(mut state) => {
                state.started = true;
                state.ext_sinks.push(sink);
                RResult::ROk(())
            }
            Err(_) => RResult::RErr("simulator state lock was poisoned".into()),
        }
    }

    fn load_batch(&mut self) -> RResult<(), RString> {
        match SimulatorState::advance(&self.state) {
            Ok(()) => RResult::ROk(()),
            Err(error) => RResult::RErr(error.into()),
        }
    }
}

struct SimulatorDiscovery;

impl DeviceDiscoveryPlugin for SimulatorDiscovery {
    fn discover(&self) -> RVec<PluginCameraDescriptionAbi> {
        vec![PluginCameraDescriptionAbi {
            serial: SIMULATOR_SERIAL.into(),
            connection: ConnectionType::Software,
        }]
        .into()
    }

    fn configuration_schema(&self) -> RString {
        RAW_EVENT_SIMULATOR_SCHEMA.into()
    }

    fn open_device(
        &self,
        _serial: abi_stable::std_types::RStr<'_>,
    ) -> RResult<DevicePluginBox, RString> {
        RResult::RErr("the simulator requires a configuration object".into())
    }

    fn open_device_with_configuration(
        &self,
        configuration: PluginConfiguration,
    ) -> RResult<DevicePluginBox, RString> {
        let schema = match PluginConfigurationSchema::parse(RAW_EVENT_SIMULATOR_SCHEMA) {
            Ok(schema) => schema,
            Err(error) => return RResult::RErr(error.to_string().into()),
        };
        if let Err(error) = schema.validate(&configuration) {
            return RResult::RErr(error.to_string().into());
        }
        match SimulatorDevice::open(&configuration) {
            Ok(device) => RResult::ROk(DevicePlugin_TO::from_value(device, TD_Opaque)),
            Err(error) => RResult::RErr(error.to_string().into()),
        }
    }
}

extern "C" fn simulator_plugin_name() -> RString {
    "openevt_simulator".into()
}

extern "C" fn create_simulator_discovery() -> DeviceDiscoveryPluginBox {
    DeviceDiscoveryPlugin_TO::from_value(SimulatorDiscovery, TD_Opaque)
}

/// Constructs the simulator plugin root module.
#[abi_stable::export_root_module]
pub fn instantiate_root_module() -> DevicePluginModuleRef {
    DevicePluginModuleVtable {
        name: simulator_plugin_name,
        create_discovery: create_simulator_discovery,
    }
    .leak_into_prefix()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_requires_a_video_and_allows_optional_overrides() {
        let schema = PluginConfigurationSchema::parse(RAW_EVENT_SIMULATOR_SCHEMA).unwrap();
        let configuration = schema.new_configuration(SIMULATOR_SERIAL);
        assert!(schema.validate(&configuration).is_err());
        assert_eq!(schema.parameters.len(), 3);
        assert!(!schema.parameters[1].required);
        assert!(!schema.parameters[2].required);
    }

    #[test]
    fn discovery_exposes_the_simulator_configuration() {
        let discovery = SimulatorDiscovery;
        assert_eq!(discovery.discover().len(), 1);
        assert!(discovery.configuration_schema().contains("video_file"));
    }
}
