use crate::hal::facilities::{
    BaseDecoderFacility, EventDecoderFacility, EventsStreamDecoderFacility,
};

pub trait RawFormatDecoder:
    EventsStreamDecoderFacility + BaseDecoderFacility + EventDecoderFacility
{
}

// Blanket implementation for any type meeting the bounds
impl<T> RawFormatDecoder for T where
    T: EventsStreamDecoderFacility + BaseDecoderFacility + EventDecoderFacility
{
}
