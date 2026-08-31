pub mod activity_noise_filter;
pub mod anti_flicker_filter;
pub mod polarity_filter;
pub mod roi_filter;
pub mod spatio_temporal_contrast;
pub mod time_surface;
pub mod trail_filter;

use crate::hal::types::EventCD;

pub trait EvtTransformer {
    fn transform_events<'a, T>(&'a self, events: Box<dyn Iterator<Item = EventCD> + 'a>) -> T;
}

pub trait EvtProcessor {
    // Takes mutable processor state and an owned iterator, returning a boxed iterator.
    // The returned iterator's lifetime is bounded by both the input stream and self.
    fn process_events<'a>(
        &'a mut self,
        events: Box<dyn Iterator<Item = EventCD> + 'a>,
    ) -> Box<dyn Iterator<Item = EventCD> + 'a>;
}

pub struct Pipeline<'a> {
    processors: &'a mut [Box<dyn EvtProcessor>],
}

impl<'a> Pipeline<'a> {
    pub fn new(processors: &'a mut [Box<dyn EvtProcessor>]) -> Self {
        Self { processors }
    }

    pub fn process<'b>(
        &'b mut self,
        events: impl Iterator<Item = EventCD> + 'b,
    ) -> Box<dyn Iterator<Item = EventCD> + 'b>
    where
        'a: 'b, // The processors must outlive the evaluation lifetime 'b
    {
        let mut stream: Box<dyn Iterator<Item = EventCD> + 'b> = Box::new(events);
        for processor in &mut *self.processors {
            stream = processor.process_events(stream);
        }
        stream
    }
}
