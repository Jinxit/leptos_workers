use std::{cell::RefCell, collections::HashMap};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use wasm_bindgen::{JsCast, JsValue};

thread_local! {
    /// Stores both the actual object, e.g. a file, and the transferable object, e.g. the array buffer.
    pub(crate) static TRANSFER_STORE_SERIALIZATION: RefCell<HashMap<u64, (JsValue, Option<JsValue>)>> = RefCell::new(HashMap::new());

    /// Just stores the actual object, the transferable object isn't referenced again.
    pub(crate) static TRANSFER_STORE_DESERIALIZATION: RefCell<js_sys::Object> = RefCell::new(js_sys::Object::new());
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub(crate) enum WorkerMsgType {
    ReqFuture,
    ReqStream,
    ReqCallback,
    ReqChannel,
    Response,
}

/// A message that is sent and received by workers.
pub(crate) struct WorkerMsg {
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
    msg_type: WorkerMsgType,
}

impl WorkerMsg {
    /// Create a new message to send.
    ///
    /// Transferables are extracted during serialization, requiring some setup,
    /// therefore must be passed as a callback.
    pub fn new(msg_type: WorkerMsgType, data: impl Serialize) -> Self {
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

                    if let Some(underlying_transferable) = underlying_transferable {
                        underlying_transferables.push(&underlying_transferable);
                    }
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
    pub fn new_null(msg_type: WorkerMsgType) -> Self {
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

    pub fn message_type(&self) -> WorkerMsgType {
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
            .expect("This WorkerMsg was decoded rather than prepared for sending, bug.");

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
    pub fn decode(value: JsValue) -> Self {
        let type_data_and_transferables = value.unchecked_into::<js_sys::Array>();
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
