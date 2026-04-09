use crossbeam::channel::{Receiver, Sender, TrySendError, bounded};
use std::any::TypeId;
use std::collections::hash_map::HashMap;
use std::error::Error;
use std::sync::{Arc, RwLock};

use crate::hal::errors::{DecoderError, DecoderProtocolViolation, SharedError};
use crate::hal::facilities::{BaseDecoderFacility, EventsStreamDecoderFacility};
use crate::hal::types::{EventCD, EventExtTrigger};
use log::{debug, warn};
use macros::derive_value;
use macros::new;

use std::ops::Deref;

/// A wrapper that returns its inner vector to a recycling channel when dropped.
pub struct PooledBuffer<T> {
    /// The underlying vector. Wrapped in an Option so it can be taken out during Drop.
    buffer: Option<Vec<T>>,
    /// The channel used to return the vector to the pool for reuse.
    return_channel: Sender<Vec<T>>,
}

/// Implements `Deref` to allow `PooledBuffer` to be treated transparently as a `&Vec<T>`.
impl<T> Deref for PooledBuffer<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        self.buffer
            .as_ref()
            .expect("Buffer is always Some until Drop")
    }
}

/// Automatically returns the cleared vector to the recycling channel when dropped.
impl<T> Drop for PooledBuffer<T> {
    fn drop(&mut self) {
        // Take ownership of the buffer out of the Option
        if let Some(mut buf) = self.buffer.take() {
            buf.clear(); // Reset length to 0, but retain capacity
            // Try to return to the pool. If the pool is full or dropped, let it deallocate.
            let _ = self.return_channel.try_send(buf);
        }
    }
}

/// Decoder for the EVT3 event data format.
/// EVT3 is commonly used by event-based vision sensors to encode timestamps and pixel coordinates efficiently.
pub struct Evt3Decoder {
    /// Thread-safe reference to the event dispatcher used to route decoded events.
    pub evt_dispatcher: Arc<EventDispatcher>,
    /// Thread-safe reference to the error dispatcher used to route errors to specific handlers.
    pub err_dispatcher: Arc<ErrorDispatcher>,
    /// The first fully decoded timestamp; used to enable time shifting if required.
    first_ts: Option<usize>,
    /// Accumulated base time offset used to reconstruct timestamps from 24bit timer events.
    time_offset: usize,
    /// The last 24-bit timestamp (t24) received, used to handle timer wrap-around.
    last_t24: usize,
    /// The last reported timestamp, saved for the get_last_timestamp method.
    last_t: usize,
    /// Flag indicating whether timestamps should be shifted relative to the first event.
    pub do_time_shift: bool,
    /// Holds a partial byte when an event word is split across buffer boundaries.
    split_byte: Option<u8>,
    /// High 16 bits of the current event timestamp.
    time_high: usize,
    /// Low 16 bits of the current event timestamp.
    time_low: usize,
    /// Y-coordinate of the current event.
    y: Option<u16>,
    /// X-coordinate of the current event.
    base_x: u16,
    /// Polarity of the event (e.g., true for ON/increase, false for OFF/decrease).
    polarity: bool,
    /// The previous decoded EVT3 word, used for state-dependent decoding.
    prev_word: Option<EVTWord>,
    /// Indicates the number of valid bits or a validity mask for the payload.
    /// Accumulates payload data (e.g., bitmasks representing multiple events in a row).
    payload_accumulator: usize,
    /// Subtype identifier for 'other' event types (e.g., external triggers, sensor markers).
    others_subtype: u16,
    /// Current bit shift index within the payload accumulator.
    payload_bit_shift: u8,
    /// A buffer of decoded CD events for batching dispatches.
    cd_buffer: Vec<EventCD>,
    /// A buffer of ext_trigger_events for batching dispatches.
    ext_trigger_buffer: Vec<EventExtTrigger>,
    /// The max address for an x coordinate
    pub max_x: u16,
    /// The max address for a y coordinate
    pub max_y: u16,
    /// Previous time_high
    prev_time_high: usize,

    // Pool Channels
    cd_pool_tx: Sender<Vec<EventCD>>,
    cd_pool_rx: Receiver<Vec<EventCD>>,
    ext_pool_tx: Sender<Vec<EventExtTrigger>>,
    ext_pool_rx: Receiver<Vec<EventExtTrigger>>,
}

impl Default for Evt3Decoder {
    fn default() -> Self {
        let (cd_pool_tx, cd_pool_rx) = bounded(32);
        let (ext_pool_tx, ext_pool_rx) = bounded(32);

        Self {
            evt_dispatcher: Default::default(),
            err_dispatcher: Default::default(),
            first_ts: Default::default(),
            time_offset: 0,
            last_t24: Default::default(),
            last_t: Default::default(),
            do_time_shift: false,
            split_byte: Default::default(),
            time_high: Default::default(),
            time_low: Default::default(),
            y: Default::default(),
            base_x: Default::default(),
            polarity: Default::default(),
            prev_word: Default::default(),
            payload_accumulator: Default::default(),
            others_subtype: Default::default(),
            payload_bit_shift: Default::default(),
            cd_buffer: Vec::with_capacity(Self::BATCH_SIZE),
            ext_trigger_buffer: Vec::with_capacity(Self::BATCH_SIZE),
            max_x: 640,
            max_y: 480,
            prev_time_high: Default::default(),
            cd_pool_tx,
            cd_pool_rx,
            ext_pool_tx,
            ext_pool_rx,
        }
    }
}

impl Evt3Decoder {
    pub fn new(max_x: u16, max_y: u16, do_time_shift: bool) -> Self {
        let mut decoder = Evt3Decoder::default();
        decoder.max_x = max_x;
        decoder.max_y = max_y;
        decoder.do_time_shift = do_time_shift;

        decoder
    }
}

/// Represents the different types of event words in an event data stream.
#[derive_value]
#[derive(new)]
enum EVTWord {
    /// X-coordinate address event.
    AddrX,
    /// Y-coordinate address event.
    AddrY,
    /// Base X vector event.
    VectBaseX,
    /// 12-bit vector event.
    Vect12,
    /// 8-bit vector event.
    Vect8,
    /// Lower bits of the timestamp.
    TimeLow,
    /// Higher bits of the timestamp.
    TimeHigh,
    /// 4-bit continued data event.
    Continued4,
    /// External trigger event.
    ExtTrigger,
    /// Other or unspecified event types.
    Others,
    /// 12-bit continued data event.
    Continued12,
}

pub struct ErrorDispatcher {
    subscribers: RwLock<HashMap<TypeId, Vec<Sender<SharedError>>>>,
    channel_capacity: usize,
}

impl Default for ErrorDispatcher {
    fn default() -> Self {
        ErrorDispatcher::new(1024)
    }
}

impl ErrorDispatcher {
    /// Initializes the dispatcher with a set capacity for all subscriber channels.
    pub fn new(channel_capacity: usize) -> Self {
        Self {
            subscribers: RwLock::new(HashMap::new()),
            channel_capacity,
        }
    }

    pub fn subscribe<T: Error + 'static>(&self) -> Receiver<SharedError> {
        let (tx, rx) = bounded(self.channel_capacity);
        let type_id = TypeId::of::<T>();

        let mut subs = self.subscribers.write().unwrap();
        subs.entry(type_id).or_default().push(tx);

        rx
    }

    pub fn unsubscribe<T: Error + 'static>(&self) -> bool {
        let type_id = TypeId::of::<T>();
        let mut subs = self.subscribers.write().unwrap();
        subs.remove(&type_id).is_some()
    }

    pub fn dispatch<T: Error + Send + Sync + 'static>(&self, error: T) {
        let type_id = TypeId::of::<T>();
        let shared_error: SharedError = Arc::new(error);

        let mut subs = self.subscribers.write().unwrap();

        if let Some(senders) = subs.get_mut(&type_id) {
            senders.retain(|tx| {
                match tx.try_send(Arc::clone(&shared_error)) {
                    Ok(_) => true,
                    // The receiver is active but the queue is full.
                    // Keep the channel registered, but drop this specific message for this consumer.
                    Err(TrySendError::Full(_)) => {
                        // Optional: Log the dropped message metric here
                        true
                    }
                    // The receiver has been dropped. Remove the sender from the vector.
                    Err(TrySendError::Disconnected(_)) => false,
                }
            });
        }
    }
}

/// A dispatcher that routes events to multiple subscribers.
/// It supports routing for both CD (Change Detection) events and External Trigger events.
pub struct EventDispatcher {
    /// Subscribers for CD (Change Detection) events.
    cd_subscribers: RwLock<Vec<Sender<Arc<PooledBuffer<EventCD>>>>>,
    /// Subscribers for external trigger events.
    ext_subscribers: RwLock<Vec<Sender<Arc<PooledBuffer<EventExtTrigger>>>>>,
}

impl Default for EventDispatcher {
    fn default() -> Self {
        EventDispatcher::new()
    }
}

impl EventDispatcher {
    /// Creates a new `EventDispatcher` with empty subscriber lists.
    fn new() -> Self {
        EventDispatcher {
            cd_subscribers: RwLock::new(Vec::new()),
            ext_subscribers: RwLock::new(Vec::new()),
        }
    }

    /// Subscribes to CD events.
    ///
    /// # Arguments
    /// * `capacity` - The maximum number of unread event batches the channel can hold.
    ///
    /// # Returns
    /// A `Receiver` channel to consume `EventCD` batches.
    fn subscribe_cd(&self, capacity: usize) -> Receiver<Arc<PooledBuffer<EventCD>>> {
        let (tx, rx) = bounded(capacity);
        self.cd_subscribers.write().unwrap().push(tx);
        rx
    }

    /// Subscribes to External Trigger events.
    ///
    /// # Arguments
    /// * `capacity` - The maximum number of unread event batches the channel can hold.
    ///
    /// # Returns
    /// A `Receiver` channel to consume `EventExtTrigger` batches.
    fn subscribe_ext(&self, capacity: usize) -> Receiver<Arc<PooledBuffer<EventExtTrigger>>> {
        let (tx, rx) = bounded(capacity);
        self.ext_subscribers.write().unwrap().push(tx);
        rx
    }

    /// Broadcasts a batch of CD events to all registered subscribers.
    ///
    /// Handles subscriber backpressure by dropping the event batch for any subscriber
    /// whose queue is full, and automatically cleans up disconnected subscribers.
    fn send_cd(&self, events: Arc<PooledBuffer<EventCD>>) {
        let mut subs = self.cd_subscribers.write().unwrap();

        subs.retain(|tx| {
            match tx.try_send(events.clone()) {
                Ok(_) => true, // Successfully queued
                Err(TrySendError::Full(_)) => {
                    // Backpressure applied: The consumer is too slow.
                    // We drop the batch for this consumer but keep them subscribed.
                    warn!(
                        "CD Event subscriber queue full. Dropping batch of {} events.",
                        events.len()
                    );
                    true
                }
                Err(TrySendError::Disconnected(_)) => {
                    // The consumer has been destroyed. Remove them from the routing table.
                    debug!("CD Event subscriber disconnected. Removing from dispatcher.");
                    false
                }
            }
        });
    }

    /// Broadcasts a batch of External Trigger events to all registered subscribers.
    ///
    /// Handles subscriber backpressure by dropping the event batch for any subscriber
    /// whose queue is full, and automatically cleans up disconnected subscribers.
    fn send_ext(&self, events: Arc<PooledBuffer<EventExtTrigger>>) {
        let mut subs = self.ext_subscribers.write().unwrap();

        subs.retain(|tx| match tx.try_send(events.clone()) {
            Ok(_) => true,
            Err(TrySendError::Full(_)) => {
                warn!("ExtTrigger subscriber queue full. Dropping batch.");
                true
            }
            Err(TrySendError::Disconnected(_)) => {
                debug!("ExtTrigger subscriber disconnected. Removing from dispatcher.");
                false
            }
        });
    }
}

/// Implements the `TryFrom` trait to safely parse an `EVTWord` from a 16-bit reference.
impl TryFrom<&u16> for EVTWord {
    type Error = DecoderProtocolViolation;

    /// Attempts to convert a 16-bit raw value into an `EVTWord`.
    ///
    /// The event type is determined by the 4 most significant bits (MSB) of the 16-bit word.
    ///
    /// # Arguments
    ///
    /// * `value` - A reference to a `u16` raw value.
    ///
    /// # Errors
    ///
    /// Returns an error `String` if the 4 MSBs do not correspond to a known `EVTWord`.
    fn try_from(value: &u16) -> Result<Self, Self::Error> {
        // Extract the 4 most significant bits by shifting right by 12.
        let msb = (value >> 12) as u8;
        match msb {
            0b0000 => Ok(Self::AddrY),
            0b0010 => Ok(Self::AddrX),
            0b0011 => Ok(Self::VectBaseX),
            0b0100 => Ok(Self::Vect12),
            0b0101 => Ok(Self::Vect8),
            0b0110 => Ok(Self::TimeLow),
            0b0111 => Ok(Self::Continued4),
            0b1000 => Ok(Self::TimeHigh),
            0b1010 => Ok(Self::ExtTrigger),
            0b1110 => Ok(Self::Others),
            0b1111 => Ok(Self::Continued12),
            // Catch any unmapped 4-bit patterns
            _ => Err(DecoderProtocolViolation::UnsupportedWord(msb.into())),
        }
    }
}

impl Evt3Decoder {
    const BATCH_SIZE: usize = 4096;
    /// Calculates and returns the continuous timestamp.
    ///
    /// Combines `time_high` and `time_low` into a 24-bit value. Handles hardware
    /// counter rollovers by maintaining a continuous `time_offset`. Logs a warning
    /// if a small backward time jump is detected, which indicates out-of-order
    /// multiplexing.
    ///
    /// # Returns
    /// * `usize` - The calculated continuous timestamp.
    #[inline(always)]
    fn current_timestamp(&mut self) -> usize {
        let t24 = (self.time_high << 12) | self.time_low;

        if t24 < self.last_t24 {
            // If the time dropped by more than half the maximum 24-bit value,
            // it is a genuine hardware counter rollover.
            if (self.last_t24 - t24) > (1 << 23) {
                self.time_offset += 1 << 24;
            } else {
                warn!("Out-of-order multiplexing");
            }
            // If the drop is small, it's just out-of-order multiplexing.
            // We do not increment the offset.
        }

        self.last_t24 = t24;

        // Return the continuous 64-bit time
        let abs_ts = self.time_offset + t24;
        if self.do_time_shift {
            let first = *self.first_ts.get_or_insert_with(|| abs_ts);
            self.last_t = abs_ts - first;
        } else {
            self.last_t = abs_ts
        }

        self.last_t
    }

    /// Flushes any pending operations by dispatching them immediately.
    pub fn flush(&mut self) {
        self.dispatch();
    }

    /// Processes a single 16-bit word from the event stream, decoding its type
    /// and payload to update the internal state or generate events.
    ///
    /// # Arguments
    /// * `word` - The 16-bit encoded data word to process.
    ///
    /// # Errors
    /// Returns `DecoderProtocolViolation` if the word sequence or values violate the protocol.
    fn process_word(&mut self, word: u16) -> Result<(), DecoderProtocolViolation> {
        // Bitmasks for extracting payloads of various sizes
        const MASK_12: u16 = 0x0FFF;
        const MASK_11: u16 = 0x07FF;
        const MASK_4: u16 = 0x000F;
        const MASK_8: u16 = 0x00FF;

        let evt_type = EVTWord::try_from(&word)?;

        match evt_type {
            EVTWord::AddrY => {
                // Decode and validate the Y coordinate
                let new_y = word & MASK_11;
                if new_y > self.max_y {
                    return Err(DecoderProtocolViolation::OutOfBoundsEventCoordinate);
                }
                self.y = Some(new_y);
                self.prev_word = Some(EVTWord::AddrY);
            }
            EVTWord::AddrX => {
                // Ensure a Y coordinate was previously received
                let y = self.y.ok_or(DecoderProtocolViolation::MissingYAddr)?;
                // Decode and validate the X coordinate
                let x = word & MASK_11;
                if x > self.max_x {
                    return Err(DecoderProtocolViolation::OutOfBoundsEventCoordinate);
                }
                // Extract polarity and generate a Contrast Detector (CD) event
                let p = ((word >> 11) & 0x01) == 1;
                let t = self.current_timestamp();
                self.cd_buffer.push(EventCD::new(x.into(), y.into(), p, t));
                self.prev_word = Some(EVTWord::AddrX);
            }
            EVTWord::VectBaseX => {
                // Establish the base X coordinate and polarity for subsequent vector events
                if self.y.is_none() {
                    return Err(DecoderProtocolViolation::MissingYAddr);
                }
                self.base_x = word & MASK_11;
                self.polarity = ((word & MASK_12) >> 11 & 0x01) != 0;
                self.prev_word = Some(EVTWord::VectBaseX);
            }
            EVTWord::Vect12 | EVTWord::Vect8 => {
                // Ensure vector events follow a valid base or previous vector event
                if !matches!(
                    self.prev_word,
                    Some(EVTWord::VectBaseX) | Some(EVTWord::Vect12)
                ) {
                    return Err(DecoderProtocolViolation::InvalidVectBase);
                }

                let is_12 = matches!(evt_type, EVTWord::Vect12);
                let bit_count = if is_12 { 12 } else { 8 };
                let mask = if is_12 { MASK_12 } else { MASK_8 };

                // Validate that the vector length won't exceed maximum X coordinate
                if self.base_x + (bit_count - 1) > self.max_x {
                    return Err(DecoderProtocolViolation::OutOfBoundsEventCoordinate);
                }

                let t = self.current_timestamp();
                let p = self.polarity;
                let x = self.base_x;
                let y = self.y.unwrap();
                let valid = word & mask;

                // Generate CD events for each active bit in the vector payload
                for i in 0..bit_count {
                    if (valid >> i) & 0x01 == 1 {
                        self.cd_buffer
                            .push(EventCD::new((x + i).into(), y.into(), p, t.into()));
                    }
                }

                // Advance the base X coordinate for the next potential vector event
                self.base_x += bit_count;
                self.prev_word = Some(evt_type);
            }
            EVTWord::ExtTrigger => {
                // Decode external trigger events (e.g., synchronization signals)
                let t = self.current_timestamp();
                let channel = ((word >> 8) & MASK_4) as usize;
                let val = word & 0x01 == 1;
                self.ext_trigger_buffer
                    .push(EventExtTrigger::new(val, t.into(), channel));
                self.prev_word = Some(EVTWord::ExtTrigger);
            }
            EVTWord::TimeLow => {
                // Update the lower bits of the current timestamp
                self.time_low = (word & MASK_12).into();
                self.prev_word = Some(EVTWord::TimeLow);
            }
            EVTWord::TimeHigh => {
                // Update the higher bits of the current timestamp and check for valid progression
                let new_time_high = (word & MASK_12) as usize;

                // Broadened wrap detection logic
                let wrap = self.time_high > 0xF00 && new_time_high < 0x0FF;

                // Enforce monotonic time progression, allowing for normal wrap-arounds
                if new_time_high < self.time_high && !wrap && self.first_ts.is_some() {
                    return Err(DecoderProtocolViolation::NonMonotonicTimeHigh);
                }
                // Check for unexpected large jumps in time
                if new_time_high > self.time_high + 10 && self.first_ts.is_some() {
                    return Err(DecoderProtocolViolation::NonContinuousTimeHigh);
                }

                self.prev_time_high = self.time_high;
                self.time_high = new_time_high;
                self.prev_word = Some(EVTWord::TimeHigh);
            }
            EVTWord::Continued12 | EVTWord::Continued4 => {
                // Accumulate multi-word payload bits for 'Others' events
                let is_12 = matches!(evt_type, EVTWord::Continued12);
                let valid_prev = if is_12 {
                    matches!(
                        self.prev_word,
                        Some(EVTWord::Others) | Some(EVTWord::Continued12)
                    )
                } else {
                    matches!(
                        self.prev_word,
                        Some(EVTWord::Others)
                            | Some(EVTWord::Continued12)
                            | Some(EVTWord::Continued4)
                    )
                };

                if !valid_prev {
                    return Err(DecoderProtocolViolation::PartialContinued);
                }

                let mask = if is_12 { MASK_12 } else { MASK_4 };
                let shift_inc = if is_12 { 12 } else { 4 };

                // Shift and accumulate the payload
                let payload = (word & mask) as usize;
                self.payload_accumulator |= payload << self.payload_bit_shift;
                self.payload_bit_shift += shift_inc;
                self.prev_word = Some(evt_type);
            }
            EVTWord::Others => {
                // Initiate a new sequence for generic/other multi-word data types
                self.others_subtype = word & MASK_12;
                self.payload_accumulator = 0;
                self.payload_bit_shift = 0;
                self.prev_word = Some(EVTWord::Others);
            }
        };

        // Dispatch events in batches to avoid unbounded buffer growth
        if self.cd_buffer.len() >= Self::BATCH_SIZE
            || self.ext_trigger_buffer.len() >= Self::BATCH_SIZE
        {
            self.dispatch();
        }

        Ok(())
    }

    /// Dispatches the currently buffered events (both CD and external triggers)
    /// to their respective event dispatchers.
    fn dispatch(&mut self) {
        // Process CD (Change Detection) buffer if it contains any events
        if !self.cd_buffer.is_empty() {
            // Retrieve an empty buffer from the pool, or allocate if the pool is dry
            let new_buffer = self
                .cd_pool_rx
                .try_recv()
                .unwrap_or_else(|_| Vec::with_capacity(Self::BATCH_SIZE));

            // Swap out the populated buffer with the empty one to avoid reallocation
            let populated_buffer = std::mem::replace(&mut self.cd_buffer, new_buffer);

            // Wrap the populated buffer so it can be returned to the pool after use
            let pooled = PooledBuffer {
                buffer: Some(populated_buffer),
                return_channel: self.cd_pool_tx.clone(),
            };

            // Send the pooled buffer to the event dispatcher
            self.evt_dispatcher.send_cd(Arc::new(pooled));
        }

        // Process external trigger buffer if it contains any events
        if !self.ext_trigger_buffer.is_empty() {
            // Retrieve an empty buffer from the external trigger pool, or allocate if dry
            let new_buffer = self
                .ext_pool_rx
                .try_recv()
                .unwrap_or_else(|_| Vec::with_capacity(Self::BATCH_SIZE));

            // Swap out the populated buffer with the empty one to avoid reallocation
            let populated_buffer = std::mem::replace(&mut self.ext_trigger_buffer, new_buffer);

            // Wrap the populated buffer so it can be returned to the pool after use
            let pooled = PooledBuffer {
                buffer: Some(populated_buffer),
                return_channel: self.ext_pool_tx.clone(),
            };

            // Send the pooled buffer to the external event dispatcher
            self.evt_dispatcher.send_ext(Arc::new(pooled));
        }
    }
}

impl BaseDecoderFacility for Evt3Decoder {
    /// Subscribes to decoder protocol violation errors.
    ///
    /// Returns a `Receiver` that yields shared errors when a violation occurs.
    fn subscribe_to_protocol_violation(&mut self) -> Receiver<SharedError> {
        self.err_dispatcher.subscribe::<DecoderProtocolViolation>()
    }

    /// Unsubscribes from decoder protocol violation errors.
    ///
    /// Returns `true` if successfully unsubscribed, `false` otherwise.
    fn unsubscribe_to_protocol_violation(&mut self) -> bool {
        self.err_dispatcher
            .unsubscribe::<DecoderProtocolViolation>()
    }

    /// Gets the size of a raw event in bytes.
    ///
    /// Always returns `Ok(2)` for EVT3 since the raw event size is fixed.
    fn get_raw_event_size_bytes(&self) -> crate::hal::facilities::FacilityResult<u8> {
        Ok(2)
    }
}

impl EventsStreamDecoderFacility for Evt3Decoder {
    fn decode(&mut self, raw_data: &[u8]) -> crate::hal::facilities::FacilityResult<()> {
        let mut data = raw_data;

        if let Some(first_byte) = self.split_byte.take() {
            // If we don't have any data yet to work with, restore the split_byte and return
            if raw_data.is_empty() {
                self.split_byte = Some(first_byte);
                return Ok(());
            }

            // Otherwise, append the split byte to the first byte in the stream.
            // This is to handle a buffer that isn't aligned with a word boundary
            let word = u16::from_le_bytes([first_byte, data[0]]);
            self.process_word(word)?;
            data = &data[1..] // Move the buffer forward past the consumed byte
        }

        let chunks = data.chunks_exact(2); // collect all 16-bit words
        let remainder = chunks.remainder(); // Save any trailing byte as a split

        for chunk in chunks {
            let lsw = chunk[0];
            let msw = chunk[1];
            let word = u16::from_le_bytes([lsw, msw]);
            self.process_word(word)?;
        }

        if !remainder.is_empty() {
            self.split_byte = Some(remainder[0]);
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
        // TODO: Figure out what it means for an event stream to be indexible and handle this
        // accordingly.
        false
    }
}
