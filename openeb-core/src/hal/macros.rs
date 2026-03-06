/// Generates getter and setter signatures for a trait.
///
/// Usage:
/// property! {
///     /// Doc comments are supported
///     frequency: u32;
/// }
#[macro_export]
macro_rules! property {
    // Match a property with attributes (docs)
    (
        $(#[$meta:meta])* $name:ident : $type:ty
    ) => {
        ::paste::paste! {
            $(#[$meta])*
            fn [<$name>](&self) -> crate::hal::errors::HalResult<$type>;

            $(#[$meta])*
            fn [<set_ $name>](&mut self, value: $type) -> crate::hal::errors::HalResult<()>;
        }
    };

    // Match multiple properties
    (
        $(
            $(#[$meta:meta])* $name:ident : $type:ty;
        )+
    ) => {
        $(
            property! {
                $(#[$meta])*
                $name : $type
            }
        )+
    };
}
