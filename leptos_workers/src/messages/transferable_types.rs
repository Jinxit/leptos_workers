use wasm_bindgen::{JsCast, JsValue};

/// A trait for implementing the types that can be transferred without copying.
///
/// Some JS values can be transferred directly to other threads, but need some special handling.
/// This trait allows configuration of those js values that must be passed separately during the postMessage call.
///
/// All basic types supporting transfers mentioned in mozilla docs are built-in,
/// along with the most common derivatives like [`js_sys::Uint8Array`]. types are implemented by leptos_workers internally,
///
/// You can implement [`TransferableType`] for custom structs wrapping a js value, as long as the underlying transfer object can be created from the js value synchronously, see the trait implementation of [`js_sys::Uint8Array`] for an example.
///
/// Transferable mozilla spec:
/// <https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Transferable_objects#supported_objects>
///
/// Example:
///
/// ```rust
/// struct MyTransferable {
///   #[serde(with = "leptos_workers::transferable")]
///   arr: js_sys::Uint8Array,
/// }
/// ```
pub trait TransferableType: std::fmt::Debug + Clone {
    /// Extract the underlying object that needs to be passed separately during the postMessage call.
    ///
    /// Transferable mozilla spec:
    /// https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Transferable_objects
    fn underlying_transfer_object(&self) -> JsValue;

    /// Convert the type from a generic js value back to the specialized type.
    fn from_js_value(value: JsValue) -> Self;

    /// Convert into a generic js value.
    fn to_js_value(&self) -> JsValue;
}

impl TransferableType for js_sys::ArrayBuffer {
    fn underlying_transfer_object(&self) -> JsValue {
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
    fn underlying_transfer_object(&self) -> JsValue {
        self.buffer().into()
    }

    fn from_js_value(value: JsValue) -> Self {
        value.unchecked_into()
    }

    fn to_js_value(&self) -> JsValue {
        self.into()
    }
}
