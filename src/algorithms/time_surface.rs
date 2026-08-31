use crate::types::{EventCoordinate, EventTimestamp};

pub struct BaseTimeSurface {
    width: EventCoordinate,
    height: EventCoordinate,
    timestamps: Vec<EventTimestamp>,
}

impl BaseTimeSurface {
    pub fn new(width: EventCoordinate, height: EventCoordinate) -> Self {
        BaseTimeSurface {
            width,
            height,
            timestamps: vec![0; (width as usize) * (height as usize) * 2],
        }
    }

    pub fn update(&mut self, x: EventCoordinate, y: EventCoordinate, p: bool, ts: EventTimestamp) {
        let index = self.get_index(x, y, p);
        self.timestamps[index] = ts;
    }

    pub fn get(&self, x: EventCoordinate, y: EventCoordinate, p: bool) -> EventTimestamp {
        self.timestamps[self.get_index(x, y, p)]
    }

    pub fn width(&self) -> EventCoordinate {
        self.width
    }
    pub fn height(&self) -> EventCoordinate {
        self.height
    }

    #[inline]
    fn get_index(&self, x: EventCoordinate, y: EventCoordinate, p: bool) -> usize {
        ((y as usize * self.width as usize) + x as usize) * 2 + (p as usize)
    }
}
