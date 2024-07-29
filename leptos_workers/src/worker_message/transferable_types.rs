use wasm_bindgen::{JsCast, JsValue};

/// A trait for implementing the types that can be transferred.
#[doc(hidden)]
pub trait TransferableType: std::fmt::Debug + Clone {
    #[allow(async_fn_in_trait)]
    /// Extract the underlying object that needs to be passed separately during the postMessage call.
    /// (if any)
    async fn underlying_transfer_object(&self) -> Option<JsValue>;

    /// Convert a generic js value back to the original type.
    fn from_js_value(value: JsValue) -> Self;

    /// Convert into a generic js value.
    fn to_js_value(&self) -> JsValue;
}

impl TransferableType for js_sys::ArrayBuffer {
    async fn underlying_transfer_object(&self) -> Option<JsValue> {
        Some(self.into())
    }

    fn from_js_value(value: JsValue) -> Self {
        value.unchecked_into()
    }

    fn to_js_value(&self) -> JsValue {
        self.into()
    }
}

impl TransferableType for js_sys::Uint8Array {
    async fn underlying_transfer_object(&self) -> Option<JsValue> {
        Some(self.buffer().into())
    }

    fn from_js_value(value: JsValue) -> Self {
        value.unchecked_into()
    }

    fn to_js_value(&self) -> JsValue {
        self.into()
    }
}
