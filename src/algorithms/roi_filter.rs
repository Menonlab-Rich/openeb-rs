use derive_new::new;

use crate::algorithms::EvtProcessor;
use crate::hal::types::{EventCD, EventCoordinate};

#[derive(new)]
pub struct RoiFilter {
    x0: EventCoordinate,
    x1: EventCoordinate,
    y0: EventCoordinate,
    y1: EventCoordinate,
}

impl EvtProcessor for RoiFilter {
    fn process_events<'a>(
        &'a mut self,
        events: Box<dyn Iterator<Item = EventCD> + 'a>,
    ) -> Box<dyn Iterator<Item = EventCD> + 'a> {
        Box::new(
            events.filter(|evt| {
                evt.x > self.x0 && evt.x < self.x1 && evt.y > self.y0 && evt.y < self.y1
            }),
        )
    }
}
