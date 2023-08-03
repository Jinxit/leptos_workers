#![warn(clippy::pedantic)]
#![allow(clippy::manual_async_fn)]
#![allow(clippy::type_complexity)]
#![allow(clippy::module_name_repetitions)]
// while API is still being solidified
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::panic)]

mod codec;
pub mod executors;
mod plumbing;
pub mod workers;

pub use leptos_workers_macro::*;
pub use plumbing::CreateWorkerError;

pub use flume::Receiver;
pub use flume::Sender;
pub use futures::stream::Stream;

#[doc(hidden)]
pub use futures::future::BoxFuture;
#[doc(hidden)]
pub use futures::stream::BoxStream;
#[doc(hidden)]
pub use futures::stream::StreamExt;
#[doc(hidden)]
pub use wasm_bindgen::prelude::wasm_bindgen;

extern crate alloc;
extern crate core;
