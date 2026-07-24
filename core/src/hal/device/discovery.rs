#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    Mipi,
    Usb,
    Network,
    Proprietary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCameraDescription {
    pub serial: String,
    pub connection: ConnectionType,
}

impl PluginCameraDescription {
    pub fn new(serial: &str, connection: ConnectionType) -> Self {
        Self {
            serial: serial.to_owned(),
            connection,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraDescription {
    pub integrator_name: String,
    pub plugin_name: String,
    pub plugin_info: PluginCameraDescription,
}

impl CameraDescription {
    pub fn new(
        integrator_name: String,
        plugin_name: String,
        plugin_info: PluginCameraDescription,
    ) -> Self {
        Self {
            integrator_name,
            plugin_name,
            plugin_info,
        }
    }
}

impl From<PluginCameraDescription> for CameraDescription {
    fn from(plugin_info: PluginCameraDescription) -> Self {
        Self {
            integrator_name: String::new(),
            plugin_name: String::new(),
            plugin_info,
        }
    }
}
