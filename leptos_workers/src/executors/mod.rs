//! This module contains implementations of executors.
//!
//! Executors know how to handle execution of different worker types.
//! They do not share an explicit trait, as their implementations may differ,
//! but the intent is that they mostly mirror the respective worker traits.

mod pool_executor;
pub use pool_executor::*;
