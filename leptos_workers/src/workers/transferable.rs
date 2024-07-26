use std::{cell::RefCell, collections::HashMap, sync::atomic::AtomicU64};

use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize, Serializer};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::MessageEvent;

/// A wrapper for a zero-copy transferable object.
/// This struct implements [`serde::Serialize`] and [`serde::Deserialize`],
/// but with trickery that will only work internally for leptos_workers.
#[derive(Clone)]
pub struct Transferable<T: TransferableType> {
    id: u64,
    value: T,
    // Set on the sender side, but None on the decoding side.
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
            underlying_transfer_object: Some(value.underlying_transfer_object().await),
            value,
        }
    }

    /// Consume the transferrable, returning the inner value.
    pub fn into_inner(self) -> T {
        self.value
    }
}

/// A trait for implementing the types that can be transferred.
#[doc(hidden)]
pub trait TransferableType:
    std::fmt::Debug + Clone + From<JsValue> + Into<JsValue> + wasm_bindgen::JsCast
{
    #[allow(async_fn_in_trait)]
    /// Extract the underlying object that needs to be passed separately during the postMessage call.
    async fn underlying_transfer_object(&self) -> JsValue;
}

impl TransferableType for js_sys::ArrayBuffer {
    async fn underlying_transfer_object(&self) -> JsValue {
        self.into()
    }
}

impl TransferableType for js_sys::Uint8Array {
    async fn underlying_transfer_object(&self) -> JsValue {
        self.buffer().into()
    }
}

thread_local! {
    /// Stores both the actual object, e.g. a file, and the transferable object, e.g. the array buffer.
    static TRANSFER_STORE_SERIALIZATION: RefCell<HashMap<u64, (JsValue, JsValue)>> = RefCell::new(HashMap::new());

    /// Just stores the actual object, the transferable object isn't referenced again.
    static TRANSFER_STORE_DESERIALIZATION: RefCell<js_sys::Object> = RefCell::new(js_sys::Object::new());
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub(crate) enum TransferableMessageType {
    ReqFuture,
    ReqStream,
    ReqCallback,
    ReqChannel,
    Response,
}

/// A message that is sent and received by workers.
pub(crate) struct TransferableMessage {
    /// The arbitrary data that was serialized, which could have had transferables, which will have been extracted.
    data: JsValue,
    /// The extracted exact transferables that might've been in the data and replaced with serializable values.
    /// Mapped by key for easy lookup.
    /// E.g. Uint8Array
    transferables: js_sys::Object,
    /// The transferable objects that must be passed separately.
    /// E.g. The ArrayBuffer backing a Uint8Array
    /// Option just because same object used on receival side, where these don't exist.
    underlying_transferables: Option<js_sys::Array>,
    msg_type: TransferableMessageType,
}

impl TransferableMessage {
    /// Create a new message to send.
    ///
    /// Transferables are extracted during serialization, requiring some setup,
    /// therefore must be passed as a callback.
    pub fn new(msg_type: TransferableMessageType, data: impl Serialize) -> Self {
        // The transfer store allows us to get access to all the transferables.
        TRANSFER_STORE_SERIALIZATION.with_borrow_mut(|store| {
            store.clear();
        });

        // THis will fill the TRANSFER_STORE_SERIALIZATION:
        let serialized =
            serde_wasm_bindgen::to_value(&data).expect("Failed to serialize message data.");

        // Extract the TRANSFER_STORE_SERIALIZATION:
        let (transferables, underlying_transferables) = TRANSFER_STORE_SERIALIZATION
            .with_borrow_mut(|store| {
                // Take it to prevent holding global references to this store.
                let store = std::mem::take(store);

                let transferables = js_sys::Object::new();
                let underlying_transferables = js_sys::Array::new();

                for (id, (value, underlying_transferable)) in store {
                    js_sys::Reflect::set(&transferables, &id.into(), &value)
                        .expect("Failed to set value in object.");

                    underlying_transferables.push(&underlying_transferable);
                }

                (transferables, underlying_transferables)
            });

        Self {
            data: serialized,
            transferables,
            underlying_transferables: Some(underlying_transferables),
            msg_type,
        }
    }

    /// Create a special message with null as the data.
    pub fn new_null(msg_type: TransferableMessageType) -> Self {
        Self {
            data: JsValue::NULL,
            transferables: js_sys::Object::new(),
            underlying_transferables: Some(js_sys::Array::new()),
            msg_type,
        }
    }

    /// Check if the message is the special null case.
    pub fn is_null(&self) -> bool {
        self.data.is_null()
    }

    pub fn message_type(&self) -> TransferableMessageType {
        self.msg_type
    }

    fn post_inner(self) -> (js_sys::Array, js_sys::Array) {
        // Store top-level objects as [serialized type, data, transferables] (underlying_transferables passed separately if needed)
        let to_send = js_sys::Array::new();
        to_send.push(
            &serde_wasm_bindgen::to_value(&self.msg_type)
                .expect("Failed to serialize message type."),
        );
        to_send.push(&self.data);
        to_send.push(&self.transferables);

        let underlying_transferables = self
            .underlying_transferables
            .expect("This TransferableMessage was decoded rather than prepared for sending, bug.");

        (to_send, underlying_transferables)
    }

    /// Post a message to a worker.
    pub fn post_to_worker(self, worker: &web_sys::Worker) {
        let (to_send, underlying_transferables) = self.post_inner();
        if underlying_transferables.length() == 0 {
            worker.post_message(&to_send).expect("post message failed");
        } else {
            worker
                .post_message_with_transfer(&to_send, &underlying_transferables)
                .expect("post message failed");
        }
    }

    /// Post a message from a worker.
    pub fn post_from_worker(self, worker: &web_sys::DedicatedWorkerGlobalScope) {
        let (to_send, underlying_transferables) = self.post_inner();
        if underlying_transferables.length() == 0 {
            worker.post_message(&to_send).expect("post message failed");
        } else {
            worker
                .post_message_with_transfer(&to_send, &underlying_transferables)
                .expect("post message failed");
        }
    }

    /// Decode a message on the receival side.
    pub fn decode(event: &MessageEvent) -> Self {
        let type_data_and_transferables = event.data().unchecked_into::<js_sys::Array>();
        let msg_type = serde_wasm_bindgen::from_value(type_data_and_transferables.get(0))
            .expect("Failed to deserialize message type.");
        let data = type_data_and_transferables.get(1);

        let transferables = type_data_and_transferables
            .get(2)
            .unchecked_into::<js_sys::Object>();

        Self {
            msg_type,
            data,
            transferables,
            underlying_transferables: None,
        }
    }

    /// Decode to the inner value, populating transferables and finishing up the transfer.
    pub fn into_inner<T: DeserializeOwned>(self) -> T {
        // Store them in the deserialization store:
        TRANSFER_STORE_DESERIALIZATION.with_borrow_mut(move |store| {
            *store = self.transferables;
        });

        // Now we've got the transferable values ready, can decode the actual data, transferables will be auto re-populated:
        let decoded =
            serde_wasm_bindgen::from_value(self.data).expect("Failed to deserialize message data.");

        // Take the store to prevent holding global references to this store.
        TRANSFER_STORE_DESERIALIZATION.with_borrow_mut(|store| {
            std::mem::take(store);
        });

        decoded
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
                    self.value.clone().into(),
                    self.underlying_transfer_object
                        .clone()
                        .expect("Underlying transfer object should be created on the sending side.")
                        .clone(),
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
            Ok(js_sys::Reflect::get(store, &id.into())
                .expect("Transferable value not found.")
                .unchecked_into::<T>())
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
