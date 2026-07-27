//! Core HAL abstractions for `openeb-rs`.
//!
//! This crate defines the shared data model and runtime interfaces used by the
//! rest of the workspace:
//!
//! - event payload types such as `EventCD` and `EventExtTrigger`
//! - decoder, stream, and device traits
//! - facility registration and type-safe retrieval
//! - event and error dispatchers
//!
//! The facility system is the primary abstraction. A device exposes a set of
//! capabilities through `FacilityHandle` values keyed by `FacilityType`. Callers
//! retrieve a handle, downcast it into the exact facility trait object they need,
//! and then operate through the trait methods. This lets a single device expose a
//! heterogeneous set of capabilities while keeping ownership, mutability, and
//! thread-safety explicit in the API.

pub mod hal;
