use macros::{derive_value, new};

#[derive_value]
#[derive(new)]
/// A decoded change-detection event.
pub struct EventCD {
    /// Horizontal pixel coordinate.
    pub x: usize,
    /// Vertical pixel coordinate.
    pub y: usize,
    /// Polarity of the change.
    pub p: bool,
    /// Event timestamp in device time units.
    pub t: usize,
}

#[derive_value]
#[derive(new)]
/// A decoded external-trigger event.
pub struct EventExtTrigger {
    /// Trigger polarity.
    pub p: bool,
    /// Event timestamp in device time units.
    pub t: usize,
    /// Trigger identifier.
    pub id: usize,
}

/// Callback that may mutate captured state.
pub type Cb<P> = Box<dyn for<'a> FnMut(P) + Send + Sync + 'static>;
/// Read-only callback.
pub type CbRo<P> = Box<dyn for<'a> Fn(P) + Send + Sync + 'static>;
/// Region represented as `(x, y, width, height)`.
pub type Region = (u32, u32, u32, u32);
/// Borrowed slice of change-detection events.
pub type EventSlice<'a> = &'a [EventCD];
/// Pixel enable/disable state used by pixel-mask facilities.
pub struct PixelMask {
    x: u32,
    y: u32,
    enabled: bool,
}

impl PixelMask {
    /// Creates a pixel mask entry.
    pub fn new(x: u32, y: u32, enabled: bool) -> Self {
        PixelMask { x, y, enabled }
    }

    /// Enables the pixel.
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disables the pixel.
    pub fn disable(&mut self) {
        self.enabled = false;
    }
}
