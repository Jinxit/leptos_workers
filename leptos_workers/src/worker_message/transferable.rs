use std::{cell::RefCell, collections::HashMap, sync::atomic::AtomicU64};

use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize, Serializer};
use wasm_bindgen::JsValue;

use super::{transferable_types::TransferableType, WorkerMsg, WorkerMsgType};

/// A wrapper for a zero-copy transferable object.
/// This struct implements [`serde::Serialize`] and [`serde::Deserialize`],
/// but with trickery that will only work internally for leptos_workers.
#[derive(Clone)]
pub struct Transferable<T: TransferableType> {
    id: u64,
    value: T,
    // Set on the sender side, depending on the type might be None, always None on the decoding side.
    underlying_transfer_object: Option<JsValue>,
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
            id: unique_id(),
            underlying_transfer_object: value.underlying_transfer_object().await,
            value,
        }
    }

    /// Consume the transferable, returning the inner value.
    pub fn into_inner(self) -> T {
        self.value
    }
}

/// Serialization store for transferable js object extraction.
struct SerStore {
    store: HashMap<u64, (JsValue, Option<JsValue>)>,
    /// Used as an assertion, panics if different to expected.
    store_id: u64,
}

impl Default for SerStore {
    fn default() -> Self {
        Self {
            store: HashMap::new(),
            store_id: unique_id(),
        }
    }
}

/// Deserialization store for transferable js object injection.
struct DeStore {
    // js (key, value) object store, no point deserializing to rust, it comes as js.
    store: JsValue,
    /// Used as an assertion, panics if different to expected.
    store_id: u64,
}

impl DeStore {
    /// Create a new store.
    fn new(store: js_sys::Object) -> Self {
        Self {
            store: store.into(),
            store_id: unique_id(),
        }
    }
}

thread_local! {
    /// Serialization global store.
    /// Stores the top-level JS object, e.g. [`js_sys::Uint8Array`],
    /// and any (optional) underlying object, e.g. the [`js_sys::ArrayBuffer`],
    /// the latter passed separately during the post message call.
    static TRANSFER_STORE_SERIALIZATION: RefCell<SerStore> = RefCell::new(SerStore::default());

    /// Just stores the actual object, the transferable object isn't referenced again.
    /// None initially to avoid instanstiating useless js objects.
    static TRANSFER_STORE_DESERIALIZATION: RefCell<Option<DeStore>> = RefCell::new(None);
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

    // Serialize the data, the custom serialize trait for transferable will fill the store, replacing the values with just their unique ids:
    let serialized =
        serde_wasm_bindgen::to_value(&data).expect("Failed to serialize message data.");

    // Extract the filled store.
    let (transferables, underlying_transferables) =
        TRANSFER_STORE_SERIALIZATION.with_borrow_mut(|store| {
            // Take it to prevent holding global references to this store.
            let store = std::mem::take(store);

            // Assert the store wasn't corrupted:
            assert_eq!(
                store.store_id, store_id,
                "Transfer store id mismatch. leptos_workers is internally broken."
            );

            let transferables = js_sys::Object::new();
            let underlying_transferables = js_sys::Array::new();

            for (id, (value, underlying_transferable)) in store.store {
                js_sys::Reflect::set(&transferables, &id.into(), &value)
                    .expect("Failed to set value in object.");

                if let Some(underlying_transferable) = underlying_transferable {
                    underlying_transferables.push(&underlying_transferable);
                }
            }

            (transferables, underlying_transferables)
        });

    WorkerMsg::construct(
        serialized,
        transferables,
        underlying_transferables,
        msg_type,
    )
}

/// WARNING: DO NOT MAKE THIS FUNCTION ASYNC, IT WILL BREAK THE STORE.
/// The thread_local store relies on 2 things:
/// - The fact serialization isn't multithreaded
/// - The fact serialization isn't concurrent (async)
///
/// This fn deserializes the data, injecting the transferable js objects in the process via the unique id mapping.
pub fn deserialize_from_worker_msg<T: DeserializeOwned>(
    transferables: js_sys::Object,
    data: JsValue,
) -> T {
    // Store them in the deserialization store for them to be read by the deserializer:
    // Also get the store_id for assertion purposes.
    let store_id = TRANSFER_STORE_DESERIALIZATION.with_borrow_mut(move |store| {
        let new_store = DeStore::new(transferables);
        let new_store_id = new_store.store_id;
        *store = Some(new_store);
        new_store_id
    });

    // Now we've got the transferable values ready, can decode the actual data, transferables will be auto re-populated:
    let decoded =
        serde_wasm_bindgen::from_value(data).expect("Failed to deserialize message data.");

    // Take the store to prevent holding global references to this store.
    TRANSFER_STORE_DESERIALIZATION.with_borrow_mut(|store| {
        // Assert the store wasn't corrupted:
        assert_eq!(
            store.as_ref().expect("Transfer store not found.").store_id,
            store_id,
            "Transfer store id mismatch. leptos_workers is internally broken."
        );

        std::mem::take(store);
    });

    decoded
}

impl<T: TransferableType> Serialize for Transferable<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Store the value, this store is re-initialised right before each request serialization, and isn't used across await points.
        TRANSFER_STORE_SERIALIZATION.with_borrow_mut(|store| {
            store.store.insert(
                self.id,
                (
                    self.value.to_js_value(),
                    self.underlying_transfer_object.clone(),
                ),
            );
        });

        // Only serialize the id:
        self.id.serialize(serializer)
    }
}

impl<'de, T: TransferableType> Deserialize<'de> for Transferable<T> {
    fn deserialize<D>(deserializer: D) -> Result<Transferable<T>, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Only the id was serialized:
        let id = u64::deserialize(deserializer)?;

        // Will be initialised with the values on the deserialization side too:
        let value = TRANSFER_STORE_DESERIALIZATION.with_borrow_mut(|store| {
            Ok(T::from_js_value(
                js_sys::Reflect::get(
                    &store.as_ref().expect("Transfer store not found.").store,
                    &id.into(),
                )
                .expect("Transferable value not found."),
            ))
        })?;

        Ok(Self {
            id,
            value,
            underlying_transfer_object: None,
        })
    }
}

/// Get a new unique id to use for referencing the transferred value.
fn unique_id() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    // (this auto rolls at u64::MAX)
    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
