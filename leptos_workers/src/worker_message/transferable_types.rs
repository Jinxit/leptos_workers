use wasm_bindgen::{JsCast, JsValue};

/// A trait for implementing the types that can be transferred.
///
/// Some JS values can be sent to/from workers via structured cloning, others via transferables.
/// This trait allows configuration of those js values that shouldn't be serialized with serde,
/// but preserved as js.
///
/// Some [`js_sys`] types are implemented by leptos_workers internally,
/// other's can be PR'd or wrapped in structs downstream in user code.
///
/// Structured clone spec:
/// <https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Structured_clone_algorithm>
///
/// Transferable spec:
/// <https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Transferable_objects#supported_objects>
pub trait TransferableType: std::fmt::Debug + Clone {
    #[allow(async_fn_in_trait)]
    /// Extract the underlying object that needs to be passed separately during the postMessage call.
    ///
    /// Transferable docs:
    /// https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Transferable_objects
    async fn underlying_transfer_object(&self) -> JsValue;

    /// Convert a generic js value back to the original type.
    fn from_js_value(value: JsValue) -> Self;

    /// Convert into a generic js value.
    fn to_js_value(&self) -> JsValue;
}

impl TransferableType for js_sys::ArrayBuffer {
    async fn underlying_transfer_object(&self) -> JsValue {
        self.into()
    }

    fn from_js_value(value: JsValue) -> Self {
        value.unchecked_into()
    }

    fn to_js_value(&self) -> JsValue {
        self.into()
    }
}

impl TransferableType for js_sys::Uint8Array {
    async fn underlying_transfer_object(&self) -> JsValue {
        self.buffer().into()
    }

    fn from_js_value(value: JsValue) -> Self {
        value.unchecked_into()
    }

    fn to_js_value(&self) -> JsValue {
        self.into()
    }
}
