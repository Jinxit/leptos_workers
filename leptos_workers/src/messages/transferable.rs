use std::{cell::RefCell, sync::atomic::AtomicU64};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use wasm_bindgen::JsValue;

use super::{transferable_types::TransferableType, WorkerMsg, WorkerMsgType};

/// A wrapper for a zero-copy transferable object.
/// This struct implements [`serde::Serialize`] and [`serde::Deserialize`],
/// but with trickery that will only work internally for leptos_workers.
#[derive(Clone)]
pub struct Transferable<T: TransferableType> {
    value: T,
    underlying_transfer_object: JsValue,
}

/// Custom as just going to show as Transferable(self.value) in the debug output.
impl<T: TransferableType> std::fmt::Debug for Transferable<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Transferable").field(&self.value).finish()
    }
}

impl<T: TransferableType> Transferable<T> {
    /// Create a new transferable object.
    ///
    /// Currently natively supports:
    /// - [`js_sys::ArrayBuffer`]
    /// - [`js_sys::Uint8Array`]
    ///
    /// Support can be added by implementing the [`TransferableType`] trait.
    pub async fn new(value: T) -> Self {
        Self {
            underlying_transfer_object: value.underlying_transfer_object().await,
            value,
        }
    }

    /// Consume the transferable, returning the inner value.
    pub fn into_inner(self) -> T {
        self.value
    }
}

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
pub fn serialize_to_worker_msg(msg_type: WorkerMsgType, data: impl Serialize) -> WorkerMsg {
    // The store should have been cleared with [`std::mem::take`] at the end of the last fn call, but just in case we'll clear it again:
    // We'll also extract the active store_id to use for assertion purposes.
    let store_id = TRANSFER_STORE_SERIALIZATION.with_borrow_mut(|store| {
        let new_store = SerStore::default();
        let new_store_id = new_store.store_id;
        let _ = std::mem::replace(store, new_store);
        new_store_id
    });

    // Serialize the data, the custom serialize trait for transferable will fill the store.
    let serialized =
        serde_wasm_bindgen::to_value(&data).expect("Failed to serialize message data.");

    // Extract the filled store.
    let underlying_transferables = TRANSFER_STORE_SERIALIZATION.with_borrow_mut(|store| {
        // Take it to prevent holding global references to this store.
        let store = std::mem::take(store);

        // Assert the store wasn't corrupted:
        assert_eq!(
            store.store_id, store_id,
            "Transfer store id mismatch. leptos_workers is internally broken."
        );

        let underlying_transferables = js_sys::Array::new();

        for underlying_transferable in store.store {
            underlying_transferables.push(&underlying_transferable);
        }

        underlying_transferables
    });

    WorkerMsg::construct(serialized, underlying_transferables, msg_type)
}

/// The wrapper that actually gets serialized by serde_wasm_bindgen, after underlying transferables have been extracted.
#[derive(Serialize, Deserialize)]
struct JsWrapped {
    #[serde(with = "serde_wasm_bindgen::preserve")]
    value: JsValue,
    #[serde(with = "serde_wasm_bindgen::preserve")]
    underlying_transfer_object: JsValue,
}

impl<T: TransferableType> Serialize for Transferable<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Store the value in the temporary global store:
        TRANSFER_STORE_SERIALIZATION.with_borrow_mut(|store| {
            store.store.push(self.underlying_transfer_object.clone());
        });
        let wrapped = JsWrapped {
            value: self.value.to_js_value(),
            underlying_transfer_object: self.underlying_transfer_object.clone(),
        };
        wrapped.serialize(serializer)
    }
}

impl<'de, T: TransferableType> Deserialize<'de> for Transferable<T> {
    fn deserialize<D>(deserializer: D) -> Result<Transferable<T>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wrapped = JsWrapped::deserialize(deserializer)?;
        Ok(Self {
            value: T::from_js_value(wrapped.value),
            underlying_transfer_object: wrapped.underlying_transfer_object,
        })
    }
}

/// Get a new unique id to use for assertions.
fn unique_id() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    // (this auto rolls at u64::MAX)
    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
