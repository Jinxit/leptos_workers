use serde::{de::DeserializeOwned, Deserialize, Serialize};
use wasm_bindgen::{JsCast, JsValue};

use super::transferable::{deserialize_from_worker_msg, serialize_to_worker_msg};

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
        // Handing off to the transferable module to keep complex logic localised.
        serialize_to_worker_msg(msg_type, data)
    }


    /// Underlying fn to construct a new message on the sender side. 
    pub fn construct(
        data: JsValue,
        transferables: js_sys::Object,
        underlying_transferables: js_sys::Array,
        msg_type: WorkerMsgType,
    ) -> Self {
        Self {
            data,
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
        deserialize_from_worker_msg(self.transferables, self.data)
    }
}
