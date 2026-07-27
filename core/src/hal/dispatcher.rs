//! Dispatchers used by decoders to publish decoded batches and errors.
//!
//! These are small fan-out utilities built on `crossbeam` channels. They are
//! lossy under backpressure: if a subscriber stops draining its
//! queue, the dispatcher drops that batch for that subscriber rather than
//! blocking the decoder.

use std::{
    any::TypeId,
    collections::HashMap,
    error::Error,
    sync::{Arc, RwLock},
};

use crate::hal::{
    errors::SharedError,
    types::{EventCD, EventExtTrigger},
};
use crossbeam::channel::{Receiver, Sender, TrySendError, bounded};
use log::{debug, warn};
use utilities::buffer::PooledBuffer;

/// Routes typed errors to subscribers that asked for that exact error type.
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
    /// Creates a dispatcher with a fixed channel capacity for all subscribers.
    pub fn new(channel_capacity: usize) -> Self {
        Self {
            subscribers: RwLock::new(HashMap::new()),
            channel_capacity,
        }
    }

    /// Subscribes to errors of type `T`.
    ///
    /// The returned receiver yields `SharedError` values that can be downcast
    /// by the consumer if needed.
    pub fn subscribe<T: Error + 'static>(&self) -> Receiver<SharedError> {
        let (tx, rx) = bounded(self.channel_capacity);
        let type_id = TypeId::of::<T>();

        let mut subs = self.subscribers.write().unwrap();
        subs.entry(type_id).or_default().push(tx);

        rx
    }

    /// Removes all subscribers for the given error type.
    pub fn unsubscribe<T: Error + 'static>(&self) -> bool {
        let type_id = TypeId::of::<T>();
        let mut subs = self.subscribers.write().unwrap();
        subs.remove(&type_id).is_some()
    }

    /// Dispatches one error value to all current subscribers of the same type.
    ///
    /// Slow subscribers drop the message but remain registered.
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
                        // A dropped-message metric can be recorded here.
                        true
                    }
                    // The receiver has been dropped. Remove the sender from the vector.
                    Err(TrySendError::Disconnected(_)) => false,
                }
            });
        }
    }
}

/// Routes decoded event batches to subscribers.
pub struct EventDispatcher {
    /// Subscribers for CD events.
    cd_subscribers: RwLock<Vec<Sender<Arc<PooledBuffer<EventCD>>>>>,
    /// Subscribers for external-trigger events.
    ext_subscribers: RwLock<Vec<Sender<Arc<PooledBuffer<EventExtTrigger>>>>>,
}

impl Default for EventDispatcher {
    fn default() -> Self {
        EventDispatcher::new()
    }
}

impl EventDispatcher {
    /// Creates an empty event dispatcher.
    pub fn new() -> Self {
        EventDispatcher {
            cd_subscribers: RwLock::new(Vec::new()),
            ext_subscribers: RwLock::new(Vec::new()),
        }
    }

    /// Subscribes to decoded CD batches.
    ///
    /// `capacity` controls how many batches can queue before the dispatcher
    /// starts dropping batches for that subscriber.
    pub fn subscribe_cd(&self, capacity: usize) -> Receiver<Arc<PooledBuffer<EventCD>>> {
        let (tx, rx) = bounded(capacity);
        self.cd_subscribers.write().unwrap().push(tx);
        rx
    }

    /// Subscribes to decoded external-trigger batches.
    pub fn subscribe_ext(&self, capacity: usize) -> Receiver<Arc<PooledBuffer<EventExtTrigger>>> {
        let (tx, rx) = bounded(capacity);
        self.ext_subscribers.write().unwrap().push(tx);
        rx
    }

    /// Broadcasts a CD batch to all subscribers.
    ///
    /// Full queues drop the batch for that subscriber. Disconnected subscribers
    /// are removed.
    pub fn send_cd(&self, events: Arc<PooledBuffer<EventCD>>) {
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

    /// Broadcasts an external-trigger batch to all subscribers.
    pub fn send_ext(&self, events: Arc<PooledBuffer<EventExtTrigger>>) {
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
