use std::{cell::RefCell, sync::atomic::AtomicU64};

use serde::{Deserializer, Serialize, Serializer};
use wasm_bindgen::JsValue;

use super::{transferable_types::TransferableType, WorkerMsg, WorkerMsgType};

/// Serialization store for underlying transferable js object extraction.
struct SerStore {
    // Underlying transferable objects that need to be transferred to another thread.
    store: Vec<JsValue>,
    /// Used as an assertion, panics if different to expected.
    store_id: u64,
}

impl Default for SerStore {
    fn default() -> Self {
        Self {
            store: vec![],
            store_id: unique_id(),
        }
    }
}

thread_local! {
    /// Serialization global store.
    /// Stores the underlying transferrable js objects, e.g. the [`js_sys::ArrayBuffer`],
    /// that need to be passed separately during the post message call.
    static TRANSFER_STORE_SERIALIZATION: RefCell<SerStore> = RefCell::new(SerStore::default());
}

/// WARNING: DO NOT MAKE THIS FUNCTION ASYNC, IT WILL BREAK THE STORE.
/// The thread_local store relies on 2 things:
/// - The fact serialization isn't multithreaded
/// - The fact serialization isn't concurrent (async)
///
/// This fn serializes the data, extracting the transferable js objects in the process,
/// mapping them to unique ids in the serialized object.
pub(crate) fn serialize_to_worker_msg(msg_type: WorkerMsgType, data: impl Serialize) -> WorkerMsg {
    // The store should have been cleared at the end of the last fn call, but just in case we'll clear it again:
    // We'll also extract the active store_id to use for assertion purposes.
    let new_store = SerStore::default();
    let new_store_id = new_store.store_id;
    TRANSFER_STORE_SERIALIZATION.replace(new_store);

    // Serialize the data, the custom serialize trait for transferable will fill the store.
    let serialized =
        serde_wasm_bindgen::to_value(&data).expect("Failed to serialize message data.");

    // Extract the filled store.
    let store = TRANSFER_STORE_SERIALIZATION.take();

    // Assert the store wasn't corrupted:
    assert_eq!(
        store.store_id, new_store_id,
        "Transfer store id mismatch. leptos_workers is internally broken."
    );

    let underlying_transferables = js_sys::Array::new();
    for underlying_transferable in store.store {
        underlying_transferables.push(&underlying_transferable);
    }

    WorkerMsg::construct(serialized, underlying_transferables, msg_type)
}

/// Used under the hood by `#[serde(with = "leptos_workers::transferable")]` to serialize transferable types.
///
/// # Errors
///
/// Will return `Err` if serde_wasm_bindgen errors.
pub fn serialize<T: TransferableType, S>(data: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    // Store the value in the temporary global store:
    TRANSFER_STORE_SERIALIZATION.with_borrow_mut(|store| {
        store.store.extend(data.underlying_transfer_objects());
    });

    serde_wasm_bindgen::preserve::serialize(&data.to_js_value(), serializer)
}

/// Used under the hood by `#[serde(with = "leptos_workers::transferable")]` to deserialize transferable types.
///
/// # Errors
///
/// Will return `Err` if serde_wasm_bindgen errors.
pub fn deserialize<'de, T: TransferableType, D>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
{
    let js_value = serde_wasm_bindgen::preserve::deserialize::<D, JsValue>(deserializer)?;
    Ok(T::from_js_value(js_value))
}

/// Get a new unique id to use for assertions.
fn unique_id() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    // (this auto rolls at u64::MAX)
    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
