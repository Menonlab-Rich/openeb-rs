//! Device registries, discovery, and plugin-facing device contracts.

pub mod device;
/// Camera connection and discovery descriptions.
pub mod discovery;
/// Host-layer parsing and validation for plugin creation schemas.
#[cfg(feature = "plugins")]
pub mod configuration;
/// Stable ABI used by dynamically loaded third-party device plugins.
#[cfg(feature = "plugins")]
pub mod plugin;

#[cfg(feature = "plugins")]
pub use configuration::{
    PluginConfigurationError, PluginConfigurationSchema, PluginParameterKind,
    PluginParameterSchema,
};
