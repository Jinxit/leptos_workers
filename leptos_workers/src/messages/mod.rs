mod post_message;
mod transferable_types;
mod worker_message;

/// The module containing the custom serialization/deserialization logic for transferable types.
/// Can be used on supported js values in structs.
///
/// Example:
///
/// ```rust
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Serialize, Deserialize)]
/// struct MyTransferable {
///   #[serde(with = "leptos_workers::transferable")]
///   arr: js_sys::Uint8Array,
/// }
/// ```
pub mod transferable;

pub use transferable_types::TransferableType;

pub(crate) use worker_message::{WorkerMsg, WorkerMsgType};
