use std::sync::atomic::AtomicU64;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use wasm_bindgen::JsValue;

use super::{
    transferable_types::TransferableType,
    worker_message::{TRANSFER_STORE_DESERIALIZATION, TRANSFER_STORE_SERIALIZATION},
};

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
    /// Currently supports:
    /// - [`js_sys::ArrayBuffer`]
    /// - [`js_sys::Uint8Array`]
    pub async fn new(value: T) -> Self {
        Self {
            id: transferrable_id(),
            underlying_transfer_object: value.underlying_transfer_object().await,
            value,
        }
    }

    /// Consume the transferrable, returning the inner value.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T: TransferableType> Serialize for Transferable<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Store the value, this store is re-initialised right before each request serialization, and isn't used across await points.
        TRANSFER_STORE_SERIALIZATION.with_borrow_mut(|store| {
            store.insert(
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
                js_sys::Reflect::get(store, &id.into()).expect("Transferable value not found."),
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
fn transferrable_id() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    // (this auto rolls at u64::MAX)
    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
