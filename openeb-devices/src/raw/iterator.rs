//! Batch-oriented iterator helpers for decoded CD events.
//!
//! `EventWindowIterator` sits on top of `RawFileReader` and lets callers consume
//! decoded events either as fixed-size batches or as time windows. Two modes are
//! available:
//!
//! - `IterSync` actively loads more raw data when the decoded queue runs dry.
//! - `IterAsync` assumes another thread is feeding the decoder and only drains
//!   the receiver.

use std::{
    marker::PhantomData,
    sync::{Arc, RwLock},
};

use crossbeam::channel::Receiver;
use openeb_core::hal::{
    errors::StreamError,
    facilities::{EventsStreamDecoderFacility, EventsStreamFacility, FacilityError},
    types::EventCD,
};
use utilities::buffer::PooledBuffer;

use crate::{raw::stream::RREventStream, types::DeviceFileError};

/// Type-state marker for an iterator that loads raw data synchronously.
pub struct IterSync;
/// Type-state marker for an iterator fed by another thread.
pub struct IterAsync;
/// Type-state marker for an iterator not yet configured for a mode.
pub struct IterUnconfigured;

/// Consumes decoded CD events in batch or time-window form.
pub struct EventWindowIterator<const BUFFER_SIZE: usize, State = IterUnconfigured> {
    stream_handle: Arc<RwLock<dyn EventsStreamFacility + Send + 'static>>,
    decoder_handle: Arc<RwLock<dyn EventsStreamDecoderFacility + Send + 'static>>,
    receiver: Receiver<Arc<PooledBuffer<EventCD>>>,
    // Holds leftover events extracted from PooledBuffers that haven't been consumed yet
    internal_buffer: std::collections::VecDeque<EventCD>,
    // Tracks the current temporal baseline for slicing fixed delta-t windows
    current_timestamp: Option<u64>,
    shape: (u32, u32),
    _state: PhantomData<State>,
}

impl<const BUFFER_SIZE: usize> EventWindowIterator<BUFFER_SIZE, IterUnconfigured> {
    /// Creates an iterator in the unconfigured state.
    ///
    /// Call `into_sync` or `into_async` before using batch/window methods.
    pub fn new(
        receiver: Receiver<Arc<PooledBuffer<EventCD>>>,
        shape: (u32, u32),
        stream_handle: Arc<RwLock<dyn EventsStreamFacility + Send + 'static>>,
        decoder_handle: Arc<RwLock<dyn EventsStreamDecoderFacility + Send + 'static>>,
    ) -> Self {
        Self {
            stream_handle,
            receiver,
            internal_buffer: std::collections::VecDeque::new(),
            current_timestamp: None,
            shape,
            decoder_handle,
            _state: PhantomData,
        }
    }

    /// Converts the iterator into synchronous mode.
    pub fn into_sync(self) -> EventWindowIterator<BUFFER_SIZE, IterSync> {
        EventWindowIterator {
            stream_handle: self.stream_handle,
            decoder_handle: self.decoder_handle,
            receiver: self.receiver,
            internal_buffer: self.internal_buffer,
            current_timestamp: self.current_timestamp,
            shape: self.shape,
            _state: PhantomData,
        }
    }

    /// Converts the iterator into asynchronous mode.
    pub fn into_async(self) -> EventWindowIterator<BUFFER_SIZE, IterAsync> {
        EventWindowIterator {
            stream_handle: self.stream_handle,
            decoder_handle: self.decoder_handle,
            receiver: self.receiver,
            internal_buffer: self.internal_buffer,
            current_timestamp: self.current_timestamp,
            shape: self.shape,
            _state: PhantomData,
        }
    }
}

pub trait BufferReplenisher {
    /// Ensures there are decoded events available in the internal buffer.
    fn replenish_buffer(&mut self) -> Result<(), DeviceFileError>;
}

// ASYNC MODE: Waits for an external thread to feed the channel.
impl<const BUFFER_SIZE: usize> BufferReplenisher for EventWindowIterator<BUFFER_SIZE, IterAsync> {
    fn replenish_buffer(&mut self) -> Result<(), DeviceFileError> {
        if self.internal_buffer.is_empty() {
            self.drain_channel_once();
        }
        Ok(())
    }
}

// SYNC MODE: Loads the batch locally first, then consumes the channel
impl<const BUFFER_SIZE: usize> BufferReplenisher for EventWindowIterator<BUFFER_SIZE, IterSync> {
    fn replenish_buffer(&mut self) -> Result<(), DeviceFileError> {
        if self.internal_buffer.is_empty() {
            while !self.try_drain_channel_once() {
                self.load_batch()?;
            }
        }
        Ok(())
    }
}

// Generic implementation block for ALL states
impl<const BUFFER_SIZE: usize, State> EventWindowIterator<BUFFER_SIZE, State> {
    /// Returns the sensor shape associated with the underlying reader.
    pub fn shape(&self) -> (u32, u32) {
        self.shape
    }

    /// Internal logic for reading channel data into `internal_buffer`.
    fn drain_channel_once(&mut self) {
        if let Ok(pooled_buffer) = self.receiver.recv() {
            self.internal_buffer
                .extend(pooled_buffer.as_ref().iter().cloned());
        }
    }

    /// Drains one already-decoded batch without blocking.
    ///
    /// Sync iterators must check the receiver before loading more raw data because
    /// a single raw buffer can decode into several pooled event batches.
    fn try_drain_channel_once(&mut self) -> bool {
        match self.receiver.try_recv() {
            Ok(pooled_buffer) => {
                self.internal_buffer
                    .extend(pooled_buffer.as_ref().iter().cloned());
                true
            }
            Err(_) => false,
        }
    }

    /// Returns the next batch of decoded CD events.
    ///
    /// The batch is capped at the iterator buffer size. If the stream ends after
    /// some events were already collected, those events are still returned.
    pub fn next_batch(&mut self) -> Result<Vec<EventCD>, DeviceFileError>
    where
        Self: BufferReplenisher,
    {
        let mut batch = Vec::with_capacity(BUFFER_SIZE);

        while batch.len() < BUFFER_SIZE {
            if self.internal_buffer.is_empty() {
                if let Err(error) = self.replenish_buffer() {
                    if !batch.is_empty() && is_end_of_file(&error) {
                        break;
                    }
                    return Err(error);
                }
                if self.internal_buffer.is_empty() {
                    break; // Stream ended
                }
            }

            if let Some(event) = self.internal_buffer.pop_front() {
                if self.current_timestamp.is_none() {
                    self.current_timestamp = Some(event.t as u64);
                }
                batch.push(event);
            }
        }

        Ok(batch)
    }

    /// Returns the next window of CD events that fall within `dt` from the
    /// current timestamp baseline.
    ///
    /// The iterator advances its internal time baseline by `dt` after each call.
    pub fn next_delta(&mut self, dt: u64) -> Result<Vec<EventCD>, DeviceFileError>
    where
        Self: BufferReplenisher,
    {
        if self.internal_buffer.is_empty() {
            self.replenish_buffer()?;
        }

        let start_ts = match self.current_timestamp {
            Some(ts) => ts,
            None => {
                if let Some(first_ev) = self.internal_buffer.front() {
                    let ts = first_ev.t as u64;
                    self.current_timestamp = Some(ts);
                    ts
                } else {
                    return Ok(Vec::new());
                }
            }
        };

        let end_ts = start_ts + dt;
        let mut window_events = Vec::new();

        loop {
            if self.internal_buffer.is_empty() {
                if let Err(error) = self.replenish_buffer() {
                    if !window_events.is_empty() && is_end_of_file(&error) {
                        break;
                    }
                    return Err(error);
                }
                if self.internal_buffer.is_empty() {
                    break;
                }
            }

            if let Some(event) = self.internal_buffer.front() {
                if (event.t as u64) < end_ts {
                    window_events.push(self.internal_buffer.pop_front().unwrap());
                } else {
                    break;
                }
            }
        }

        self.current_timestamp = Some(end_ts);
        Ok(window_events)
    }
}

fn is_end_of_file(error: &DeviceFileError) -> bool {
    matches!(
        error,
        DeviceFileError::EOF()
            | DeviceFileError::FacilityError(FacilityError::Stream(StreamError::EndOfFile))
    )
}

impl<const BUFFER_SIZE: usize> EventWindowIterator<BUFFER_SIZE, IterSync> {
    /// Loads and decodes one raw buffer into the iterator's event channel.
    pub fn load_batch(&mut self) -> Result<(), DeviceFileError> {
        let mut stream_facility = self
            .stream_handle
            .as_ref()
            .try_write()
            .map_err(|_| DeviceFileError::LockError)?;

        let mut decoder_facility = self
            .decoder_handle
            .as_ref()
            .try_write()
            .map_err(|_| DeviceFileError::LockError)?;

        let stream = crate::facility_downcast_mut!(stream_facility, RREventStream<BUFFER_SIZE>)?;
        let (buffer, _) = stream.poll_buffer()?;
        decoder_facility.decode(buffer)?;
        Ok(())
    }
}
