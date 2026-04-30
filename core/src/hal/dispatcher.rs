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
    pub fn new() -> Self {
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
    pub fn subscribe_cd(&self, capacity: usize) -> Receiver<Arc<PooledBuffer<EventCD>>> {
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
    pub fn subscribe_ext(&self, capacity: usize) -> Receiver<Arc<PooledBuffer<EventExtTrigger>>> {
        let (tx, rx) = bounded(capacity);
        self.ext_subscribers.write().unwrap().push(tx);
        rx
    }

    /// Broadcasts a batch of CD events to all registered subscribers.
    ///
    /// Handles subscriber backpressure by dropping the event batch for any subscriber
    /// whose queue is full, and automatically cleans up disconnected subscribers.
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

    /// Broadcasts a batch of External Trigger events to all registered subscribers.
    ///
    /// Handles subscriber backpressure by dropping the event batch for any subscriber
    /// whose queue is full, and automatically cleans up disconnected subscribers.
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
