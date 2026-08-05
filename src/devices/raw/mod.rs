mod decoder;
mod device;
mod facilities;
mod index;
mod iterator;
#[cfg(feature = "plugins")]
pub mod plugin;
mod reader;
mod stream;

pub use decoder::RREventStreamDecoder;
pub use iterator::*;
#[cfg(feature = "plugins")]
pub use plugin::RawFilePlugin;
pub(crate) use reader::RawFileReader;

#[cfg(test)]
mod tests;
