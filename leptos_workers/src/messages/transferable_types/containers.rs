use std::collections::HashMap;

use js_sys::Reflect;
use wasm_bindgen::{JsCast, JsValue};

use super::TransferableType;

impl<T: TransferableType> TransferableType for Option<T> {
    fn underlying_transfer_objects(&self) -> Vec<JsValue> {
        match self {
            Some(t) => t.underlying_transfer_objects(),
            None => vec![],
        }
    }

    fn from_js_value(value: JsValue) -> Self {
        if value.is_null() {
            None
        } else {
            Some(T::from_js_value(value))
        }
    }

    fn to_js_value(&self) -> JsValue {
        match self {
            Some(t) => t.to_js_value(),
            None => JsValue::NULL,
        }
    }
}

impl<T: TransferableType, S: ::std::hash::BuildHasher + Clone + Default> TransferableType for HashMap<String, T, S> {
    fn underlying_transfer_objects(&self) -> Vec<JsValue> {
        self.values()
            .flat_map(T::underlying_transfer_objects)
            .collect()
    }

    fn from_js_value(value: JsValue) -> Self {
        let obj = value.unchecked_ref::<js_sys::Object>();
        let mut map = HashMap::default();
        let js_entries = js_sys::Object::entries(obj);
        for entry in js_entries.iter() {
            let entry = entry.unchecked_into::<js_sys::Array>();
            let key = entry
                .get(0)
                .as_string()
                .expect("Failed to get js object key during worker message transfer.");
            let value = T::from_js_value(entry.get(1));
            map.insert(key, value);
        }
        map
    }

    fn to_js_value(&self) -> JsValue {
        let obj = js_sys::Object::new();
        for (key, value) in self {
            Reflect::set(&obj, &key.into(), &value.to_js_value())
                .expect("Failed to set js object key during worker message transfer.");
        }
        obj.into()
    }
}

impl<T: TransferableType> TransferableType for Vec<T> {
    fn underlying_transfer_objects(&self) -> Vec<JsValue> {
        self.iter()
            .flat_map(T::underlying_transfer_objects)
            .collect()
    }

    fn from_js_value(value: JsValue) -> Self {
        value
            .unchecked_ref::<js_sys::Array>()
            .iter()
            .map(|v| T::from_js_value(v))
            .collect()
    }

    fn to_js_value(&self) -> JsValue {
        let arr = js_sys::Array::new();
        for t in self {
            arr.push(&t.to_js_value());
        }
        arr.into()
    }
}
