use serde::{de::DeserializeOwned, Deserialize, Serialize};
use wasm_bindgen::{JsCast, JsValue};

use super::transferable::serialize_to_worker_msg;
use crate::messages::post_message::PostMessage;

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
    /// The arbitrary data that was serialized, will contain the top-level transferable js objects.
    data: JsValue,
    /// The underlying transferable objects that must be passed separately.
    ///
    /// E.g. The [`js_sys::ArrayBuffer`] backing a [`js_sys::Uint8Array`]
    ///
    /// Option just because the same object used on the receiving side, where these don't exist.    
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
        underlying_transferables: js_sys::Array,
        msg_type: WorkerMsgType,
    ) -> Self {
        Self {
            data,
            underlying_transferables: Some(underlying_transferables),
            msg_type,
        }
    }

    /// Create a special message with null as the data.
    /// This is used as a marker to signal the end of streams across the worker boundary.
    pub fn new_null(msg_type: WorkerMsgType) -> Self {
        Self {
            data: JsValue::NULL,
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

    /// Post a message from one thread to another.
    pub fn post(self, worker: &impl PostMessage) {
        // Store top-level objects as [serialized type, data] (underlying_transferables passed separately if needed)
        let to_send = js_sys::Array::new();
        to_send.push(
            &serde_wasm_bindgen::to_value(&self.msg_type)
                .expect("Failed to serialize message type."),
        );
        to_send.push(&self.data);

        let underlying_transferables = self
            .underlying_transferables
            .expect("This WorkerMsg was decoded rather than prepared for sending, bug.");

        if underlying_transferables.length() == 0 {
            worker.post_message(&to_send);
        } else {
            worker.post_message_with_transfer(&to_send, &underlying_transferables);
        }
    }

    /// Decode a message on the receiving side.
    pub fn decode(value: JsValue) -> Self {
        let type_data_and_transferables = value.unchecked_into::<js_sys::Array>();
        let msg_type = serde_wasm_bindgen::from_value(type_data_and_transferables.get(0))
            .expect("Failed to deserialize message type.");
        let data = type_data_and_transferables.get(1);

        Self {
            msg_type,
            data,
            underlying_transferables: None,
        }
    }

    /// Decode to the inner value, finishing up the transfer.
    pub fn into_inner<T: DeserializeOwned>(self) -> T {
        serde_wasm_bindgen::from_value(self.data).expect("Failed to deserialize message data.")
    }
}
