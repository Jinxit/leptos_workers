use crate::codec;
use crate::workers::web_worker::WebWorker;
use futures::future::BoxFuture;
use std::cell::RefCell;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;

pub trait FutureWorker: WebWorker {
    fn run(request: Self::Request) -> BoxFuture<'static, Self::Response>;
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct FutureWorkerFn {
    pub(crate) path: &'static str,
    pub(crate) function: fn(&Vec<u8>) -> BoxFuture<'static, JsValue>,
}

impl FutureWorkerFn {
    #[must_use]
    pub fn new<W: FutureWorker>() -> Self {
        Self {
            path: W::path(),
            function: move |request| {
                let request = codec::from_slice(&request[..]).expect("byte deserialization error");
                Box::pin(async move {
                    let response = W::run(request).await;
                    serde_wasm_bindgen::to_value(&response).expect("js serialization error")
                })
            },
        }
    }
}

mod private {
    use crate::workers::future_worker::{FutureWorkerFn, FUTURE_WORKER_FN};
    use js_sys::global;
    use wasm_bindgen::prelude::wasm_bindgen;
    use wasm_bindgen::JsValue;
    use web_sys::DedicatedWorkerGlobalScope;

    #[wasm_bindgen]
    pub fn register_future_worker(future_worker: &FutureWorkerFn) {
        console_error_panic_hook::set_once();

        let worker_scope: DedicatedWorkerGlobalScope = JsValue::from(global()).into();
        if worker_scope.name() == future_worker.path {
            let cell = FUTURE_WORKER_FN
                .lock()
                .expect("failed to lock FUTURE_WORKER_FN");
            let mut opt = cell.borrow_mut();
            *opt = Some(future_worker.clone());
        }
    }
}

pub(crate) static FUTURE_WORKER_FN: Mutex<RefCell<Option<FutureWorkerFn>>> =
    Mutex::new(RefCell::new(None));
