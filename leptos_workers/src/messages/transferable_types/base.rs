use wasm_bindgen::{JsCast, JsValue};

/// A trait for implementing the JS types that can be transferred to other threads without copying.
/// 
/// Such objects utilise the separate second argument of the postMessage webworker API dedicated to transferable objects.
///
/// All basic types supporting transfers mentioned in mozilla docs are built-in,
/// along with the most common derivatives like [`js_sys::Uint8Array`].
///
/// [`Vec<T>`], [`std::collections::HashMap<String, T>`], [`Option<T>`] are also implemented, as long as T implements [`TransferableType`].
/// [`JsValue`] is implemented as a fallback for downstream unimplemented types that are known to work with transferables.
///
/// You can implement [`TransferableType`] for custom structs wrapping a js value, as long as the underlying transfer object can be created from the js value synchronously, see the trait implementation of [`js_sys::Uint8Array`] for an example.
///
/// Transferable mozilla spec:
/// <https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Transferable_objects#supported_objects>
///
/// Example:
///
/// ```rust
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Serialize, Deserialize)]
/// struct MyTransferable {
///   #[serde(with = "leptos_workers::transferable")]
///   arr: js_sys::Uint8Array,
/// }
/// ```
pub trait TransferableType: std::fmt::Debug + Clone {
    /// Extract the underlying object that needs to be passed separately during the postMessage call.
    ///
    /// A vec must be returned, as this trait supports multiple transferable objects, which must be passed to postMessage flat.
    ///
    /// Transferable mozilla spec:
    /// https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Transferable_objects
    fn underlying_transfer_objects(&self) -> Vec<JsValue>;

    /// Convert into a generic js value.
    fn to_js_value(&self) -> JsValue;

    /// Convert the type from a generic js value back to the specialized type.
    fn from_js_value(value: JsValue) -> Self;
}

impl TransferableType for js_sys::ArrayBuffer {
    fn underlying_transfer_objects(&self) -> Vec<JsValue> {
        vec![self.into()]
    }

    fn from_js_value(value: JsValue) -> Self {
        value.unchecked_into()
    }

    fn to_js_value(&self) -> JsValue {
        self.into()
    }
}
