use wasm_bindgen::{JsCast, JsValue};

use super::TransferableType;

/// If the user had a random type they knew worked with transferables but leptos_workers haven't implemented a type for,
/// they can cast it to a JsValue and use this implementation.
impl TransferableType for JsValue {
    fn underlying_transfer_objects(&self) -> Vec<JsValue> {
        vec![self.clone()]
    }

    fn from_js_value(value: JsValue) -> Self {
        value
    }

    fn to_js_value(&self) -> JsValue {
        self.clone()
    }
}

impl TransferableType for js_sys::Uint8Array {
    fn underlying_transfer_objects(&self) -> Vec<JsValue> {
        vec![self.buffer().into()]
    }

    fn from_js_value(value: JsValue) -> Self {
        value.unchecked_into()
    }

    fn to_js_value(&self) -> JsValue {
        self.into()
    }
}
