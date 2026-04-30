use crossbeam::channel::{Receiver, Sender, bounded};
use std::sync::Arc;

use crate::hal::decoders::buffer::PooledBuffer;
use crate::hal::dispatcher::{ErrorDispatcher, EventDispatcher};
use crate::hal::errors::SharedError;
use crate::hal::facilities::{
    BaseDecoderFacility, EventDecoderFacility, EventsStreamDecoderFacility, FacilityResult,
};
use crate::hal::types::{EventCD, EventExtTrigger};

pub struct Evt2Decoder {
    pub evt_dispatcher: Arc<EventDispatcher>,
    pub err_dispatcher: Arc<ErrorDispatcher>,

    // Time tracking
    first_ts: Option<usize>,
    last_t: usize,
    time_high: u32,
    pub do_time_shift: bool,

    // Geometry
    pub max_x: u16,
    pub max_y: u16,

    // Buffers and Pools
    split_bytes: Vec<u8>,
    cd_buffer: Vec<EventCD>,
    ext_trigger_buffer: Vec<EventExtTrigger>,

    cd_pool_tx: Sender<Vec<EventCD>>,
    cd_pool_rx: Receiver<Vec<EventCD>>,
    ext_pool_tx: Sender<Vec<EventExtTrigger>>,
    ext_pool_rx: Receiver<Vec<EventExtTrigger>>,
}

impl Evt2Decoder {
    const BATCH_SIZE: usize = 4096;

    // EVT2 Bitmasks
    const TIME_LOW_MASK: u32 = 0x3F; // 6 bits
    const X_MASK: u32 = 0x7FF; // 11 bits
    const Y_MASK: u32 = 0x7FF; // 11 bits
    const TIME_HIGH_MASK: u32 = 0xFFFFFFF; // 28 bits
    const TRIGGER_ID_MASK: u32 = 0x1F; // 5 bits

    pub fn new(max_x: u16, max_y: u16, do_time_shift: bool) -> Self {
        let (cd_pool_tx, cd_pool_rx) = bounded(32);
        let (ext_pool_tx, ext_pool_rx) = bounded(32);

        Self {
            evt_dispatcher: Default::default(),
            err_dispatcher: Default::default(),
            first_ts: None,
            last_t: 0,
            time_high: 0,
            do_time_shift,
            max_x,
            max_y,
            split_bytes: Vec::with_capacity(4),
            cd_buffer: Vec::with_capacity(Self::BATCH_SIZE),
            ext_trigger_buffer: Vec::with_capacity(Self::BATCH_SIZE),
            cd_pool_tx,
            cd_pool_rx,
            ext_pool_tx,
            ext_pool_rx,
        }
    }

    #[inline(always)]
    fn current_timestamp(&mut self, time_low: u32) -> usize {
        // Concatenate the 28-bit time_high with the 6-bit time_low
        let abs_ts = ((self.time_high as usize) << 6) | (time_low as usize);

        if self.do_time_shift {
            let first = *self.first_ts.get_or_insert(abs_ts);
            self.last_t = abs_ts.saturating_sub(first);
        } else {
            self.last_t = abs_ts;
        }

        self.last_t
    }

    fn process_word(&mut self, word: u32) {
        let evt_type = word >> 28;

        match evt_type {
            0x0 | 0x1 => {
                // 0x0 = CD_OFF (polarity 0), 0x1 = CD_ON (polarity 1)
                let p = evt_type == 0x1;
                let time_low = (word >> 22) & Self::TIME_LOW_MASK;
                let x = (word >> 11) & Self::X_MASK;
                let y = word & Self::Y_MASK;

                if x <= self.max_x as u32 && y <= self.max_y as u32 {
                    let t = self.current_timestamp(time_low);
                    self.cd_buffer
                        .push(EventCD::new(x as usize, y as usize, p, t));
                }
            }
            0x8 => {
                // 0x8 = EVT_TIME_HIGH
                self.time_high = word & Self::TIME_HIGH_MASK;
            }
            0xA => {
                // 0xA = EXT_TRIGGER
                let time_low = (word >> 22) & Self::TIME_LOW_MASK;
                let id = (word >> 8) & Self::TRIGGER_ID_MASK;
                let value = (word & 0x01) == 1;
                let t = self.current_timestamp(time_low);

                self.ext_trigger_buffer
                    .push(EventExtTrigger::new(value, t, id as usize));
            }
            _ => {
                // 0xE (OTHERS) and 0xF (CONTINUED) are vendor-specific and ignored by default
            }
        }

        if self.cd_buffer.len() >= Self::BATCH_SIZE
            || self.ext_trigger_buffer.len() >= Self::BATCH_SIZE
        {
            self.dispatch();
        }
    }

    fn dispatch(&mut self) {
        if !self.cd_buffer.is_empty() {
            let new_buffer = self
                .cd_pool_rx
                .try_recv()
                .unwrap_or_else(|_| Vec::with_capacity(Self::BATCH_SIZE));

            let populated_buffer = std::mem::replace(&mut self.cd_buffer, new_buffer);

            let pooled = PooledBuffer {
                buffer: Some(populated_buffer),
                return_channel: self.cd_pool_tx.clone(),
            };

            self.add_event_buffer(Arc::new(pooled));
        }

        if !self.ext_trigger_buffer.is_empty() {
            let new_buffer = self
                .ext_pool_rx
                .try_recv()
                .unwrap_or_else(|_| Vec::with_capacity(Self::BATCH_SIZE));

            let populated_buffer = std::mem::replace(&mut self.ext_trigger_buffer, new_buffer);

            let pooled = PooledBuffer {
                buffer: Some(populated_buffer),
                return_channel: self.ext_pool_tx.clone(),
            };

            self.evt_dispatcher.send_ext(Arc::new(pooled));
        }
    }
}

impl EventsStreamDecoderFacility for Evt2Decoder {
    fn decode(&mut self, raw_data: &[u8]) -> FacilityResult<()> {
        let mut data = raw_data;

        // Process any leftover bytes from the previous chunk
        if !self.split_bytes.is_empty() {
            let needed = 4 - self.split_bytes.len();
            if data.len() < needed {
                self.split_bytes.extend_from_slice(data);
                return Ok(());
            }

            self.split_bytes.extend_from_slice(&data[..needed]);

            // Unpack as little-endian 32-bit word
            let word = u32::from_le_bytes(self.split_bytes.as_slice().try_into().unwrap());
            self.process_word(word);

            self.split_bytes.clear();
            data = &data[needed..];
        }

        // Process exact 32-bit chunks
        let chunks = data.chunks_exact(4);
        let remainder = chunks.remainder();

        for chunk in chunks {
            let word = u32::from_le_bytes(chunk.try_into().unwrap());
            self.process_word(word);
        }

        // Store any trailing bytes for the next decode call
        if !remainder.is_empty() {
            self.split_bytes.extend_from_slice(remainder);
        }

        self.dispatch();
        Ok(())
    }

    fn get_last_timestamp(&self) -> usize {
        self.last_t
    }

    fn get_timestamp_shift(&self) -> Option<usize> {
        self.first_ts
    }

    fn is_time_shifting_enabled(&self) -> bool {
        self.do_time_shift
    }

    fn reset_last_timestamp(&mut self, timestamp: usize) {
        self.last_t = timestamp;
    }

    fn reset_timestamp_shift(&mut self, shift: usize) {
        self.first_ts = Some(shift);
    }

    fn is_decoded_event_stream_indexable(&self) -> bool {
        false
    }
}

impl BaseDecoderFacility for Evt2Decoder {
    fn subscribe_to_protocol_violation(&mut self) -> Receiver<SharedError> {
        self.err_dispatcher.subscribe::<SharedError>()
    }

    fn get_raw_event_size_bytes(&self) -> FacilityResult<u8> {
        Ok(4) // EVT2 is strictly 32-bit / 4-byte words
    }
}

impl EventDecoderFacility for Evt2Decoder {
    fn subscribe_to_event_buffer(&mut self) -> Receiver<Arc<PooledBuffer<EventCD>>> {
        self.evt_dispatcher.subscribe_cd(2048)
    }

    fn add_event_buffer(&mut self, range: Arc<PooledBuffer<EventCD>>) {
        self.evt_dispatcher.send_cd(range);
    }
}
