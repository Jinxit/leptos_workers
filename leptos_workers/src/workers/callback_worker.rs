use crate::codec;
use crate::workers::web_worker::WebWorker;
use alloc::rc::Rc;
use std::cell::RefCell;
use std::sync::Mutex;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

pub trait CallbackWorker: WebWorker {
    fn stream_callback(request: Self::Request, callback: Box<dyn Fn(Self::Response)>);
}

#[wasm_bindgen(inspectable)]
#[derive(Clone)]
pub struct CallbackWorkerFn {
    pub(crate) path: &'static str,
    pub(crate) function: fn(&Vec<u8>, Box<dyn Fn(JsValue)>),
}

impl CallbackWorkerFn {
    #[must_use]
    pub fn new<W: CallbackWorker>() -> Self {
        Self {
            path: W::path(),
            function: move |request, callback| {
                let callback = Rc::new(callback);
                let callback2 = callback.clone();
                let request = codec::from_slice(&request[..]).expect("byte deserialization error");
                W::stream_callback(
                    request,
                    Box::new(move |response| {
                        let value = serde_wasm_bindgen::to_value(&response)
                            .expect("js serialization error");
                        callback(value);
                    }),
                );
                callback2(JsValue::NULL);
            },
        }
    }
}

mod private {
    use crate::workers::callback_worker::{CallbackWorkerFn, CALLBACK_WORKER_FN};
    use js_sys::global;
    use wasm_bindgen::prelude::wasm_bindgen;
    use wasm_bindgen::JsValue;
    use web_sys::DedicatedWorkerGlobalScope;

    #[wasm_bindgen]
    pub fn register_callback_worker(callback_worker: &CallbackWorkerFn) {
        console_error_panic_hook::set_once();

        let worker_scope: DedicatedWorkerGlobalScope = JsValue::from(global()).into();
        if worker_scope.name() == callback_worker.path {
            let cell = CALLBACK_WORKER_FN
                .lock()
                .expect("failed to lock CALLBACK_WORKER_FN");
            let mut opt = cell.borrow_mut();
            *opt = Some(callback_worker.clone());
        }
    }
}

pub(crate) static CALLBACK_WORKER_FN: Mutex<RefCell<Option<CallbackWorkerFn>>> =
    Mutex::new(RefCell::new(None));
