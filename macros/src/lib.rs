pub use derive_new::new;
pub use local_proc_macros::derive_value;
pub use paste;

#[macro_export]
macro_rules! property {
    // Match read-only properties: `ro identifier: type;`
    (ro $name:ident : $ty:ty ; $($rest:tt)*) => {
        paste::paste! {
            fn [<get_ $name>](&self) -> FacilityResult<$ty>;
        }
        property!($($rest)*);
    };

    // Match read-write properties: `identifier: type;`
    ($name:ident : $ty:ty ; $($rest:tt)*) => {
        paste::paste! {
            fn [<get_ $name>](&self) -> FacilityResult<$ty>;
            fn [<set_ $name>](&mut self, value: $ty) -> FacilityResult<()>;
        }
        property!($($rest)*);
    };

    // Base case: stop recursion when empty
    () => {};
}

#[macro_export]
macro_rules! pack_facility {
    // Matches mutable facilities and applies Arc<Mutex<...>>
    (mut $variant:ident, $instance:expr) => {
        FacilityHandle::$variant(std::sync::Arc::new(std::sync::RwLock::new($instance)))
    };

    // Matches read-only facilities and applies Arc<...>
    (ro $variant:ident, $instance:expr) => {
        FacilityHandle::$variant(std::sync::Arc::new($instance))
    };
}
