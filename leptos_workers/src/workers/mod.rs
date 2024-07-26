//! This module contains all the available flavours of workers.

mod callback_worker;
mod channel_worker;
mod future_worker;
mod stream_worker;
mod transferable;
mod web_worker;

pub use callback_worker::*;
pub use channel_worker::*;
pub use future_worker::*;
pub use stream_worker::*;
pub use transferable::*;
pub use web_worker::*;
