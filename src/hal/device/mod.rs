pub mod device;
/// Camera connection and discovery descriptions.
pub mod discovery;
/// Stable ABI used by dynamically loaded third-party device plugins.
#[cfg(feature = "plugins")]
pub mod plugin;
