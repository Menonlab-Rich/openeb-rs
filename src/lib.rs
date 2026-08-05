//! OpenEB-inspired event camera abstractions and file-backed devices.
//!
//! `openevt` is independently maintained and is not endorsed or sponsored by
//! Prophesee.
//!
//! The core HAL is always available. File-backed raw devices are enabled by
//! the `devices` feature; the `all` feature enables every optional component.

pub use derive_new::new;
pub use paste;

// Compatibility aliases keep the folded modules' internal paths stable while
// allowing downstream code to use this crate as one package.
extern crate self as macros;
extern crate self as openevt_core;
extern crate self as utilities;

/// Declares getter/setter methods for facility properties.
#[macro_export]
macro_rules! property {
    (ro $name:ident : $ty:ty ; $($rest:tt)*) => {
        paste::paste! {
            fn [<get_ $name>](&self) -> FacilityResult<$ty>;
        }
        property!($($rest)*);
    };
    ($name:ident : $ty:ty ; $($rest:tt)*) => {
        paste::paste! {
            fn [<get_ $name>](&self) -> FacilityResult<$ty>;
            fn [<set_ $name>](&mut self, value: $ty) -> FacilityResult<()>;
        }
        property!($($rest)*);
    };
    () => {};
}

/// Wraps a concrete facility in the matching `FacilityHandle` variant.
#[macro_export]
macro_rules! pack_facility {
    (mut $variant:ident, $instance:expr) => {
        FacilityHandle::$variant(std::sync::Arc::new(std::sync::RwLock::new($instance)))
    };
    (ro $variant:ident, $instance:expr) => {
        FacilityHandle::$variant(std::sync::Arc::new($instance))
    };
}

/// Recyclable buffers used by event decoders and dispatchers.
pub mod buffer;
#[cfg(feature = "framegen")]
pub mod framegen;
/// Shared HAL abstractions.
pub mod hal;

#[cfg(feature = "devices")]
#[path = "devices/device_macros.rs"]
pub mod device_macros;
#[cfg(feature = "devices")]
#[path = "devices/header.rs"]
pub mod header;
#[cfg(feature = "devices")]
#[path = "devices/raw/mod.rs"]
mod raw;
#[cfg(feature = "devices")]
#[path = "devices/types.rs"]
pub mod types;

#[cfg(feature = "python")]
pub mod python;

#[cfg(all(feature = "devices", feature = "plugins"))]
pub use raw::RawFilePlugin;
#[cfg(feature = "devices")]
pub use raw::{BufferReplenisher, EventWindowIterator, IterAsync, IterSync, RREventStreamDecoder};
