use crate::algorithms::EvtProcessor;
use crate::hal::types::EventCD;
use derive_new::new;

#[derive(new)]
pub struct PolarityFilter {
    polarity: bool,
}

impl EvtProcessor for PolarityFilter {
    fn process_events<'a>(
        &'a mut self,
        events: Box<dyn Iterator<Item = EventCD> + 'a>,
    ) -> Box<dyn Iterator<Item = EventCD> + 'a> {
        Box::new(events.filter(|evt| evt.p == self.polarity))
    }
}
