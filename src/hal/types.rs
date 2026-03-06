pub struct Event {
    x: u32,
    y: u32,
    p: u32,
    t: usize,
}

pub type Cb<P> = Box<dyn for<'a> FnMut(P) + Send + Sync + 'static>;
pub type CbRo<P> = Box<dyn for<'a> Fn(P) + Send + Sync + 'static>;
pub type Region = (u32, u32, u32, u32);
pub type EventSlice<'a> = &'a [Event];
pub struct PixelMask {
    x: u32,
    y: u32,
    enabled: bool,
}

impl PixelMask {
    pub fn new(x: u32, y: u32, enabled: bool) -> Self {
        PixelMask { x, y, enabled }
    }

    pub fn enable(&mut self) {
        self.enabled = true;
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }
}
