use crate::{
    algorithms::{EvtProcessor, time_surface::BaseTimeSurface},
    types::{EventCoordinate, EventTimestamp},
};

pub struct ActivityNoiseFilter {
    dt: EventTimestamp,
    surface: BaseTimeSurface,
}

impl ActivityNoiseFilter {
    pub fn new(width: EventCoordinate, height: EventCoordinate, dt: EventTimestamp) -> Self {
        Self {
            dt,
            surface: BaseTimeSurface::new(width, height),
        }
    }
}

impl EvtProcessor for ActivityNoiseFilter {
    fn process_events<'a>(
        &'a mut self,
        events: Box<dyn Iterator<Item = crate::types::EventCD> + 'a>,
    ) -> Box<dyn Iterator<Item = crate::types::EventCD> + 'a> {
        Box::new(events.filter(move |evt| {
            let x = evt.x as isize;
            let y = evt.y as isize;
            let mut valid = false;

            // Search 3x3 neighborhood for a supporting event of the same polarity within dt
            'outer: for dy in -1..=1 {
                for dx in -1..=1 {
                    let nx = x + dx;
                    let ny = y + dy;

                    if nx >= 0
                        && nx < self.surface.width() as isize
                        && ny >= 0
                        && ny < self.surface.height() as isize
                    {
                        let prev_t =
                            self.surface
                                .get(nx as EventCoordinate, ny as EventCoordinate, evt.p);

                        if prev_t > 0 && evt.t.saturating_sub(prev_t) <= self.dt {
                            valid = true;
                            break 'outer;
                        }
                    }
                }
            }

            // Always update the surface with the latest incoming timestamp for this pixel
            self.surface.update(evt.x, evt.y, evt.p, evt.t);

            valid
        }))
    }
}
