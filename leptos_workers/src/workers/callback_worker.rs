use crate::workers::web_worker::WebWorker;
use alloc::rc::Rc;
use futures::future::LocalBoxFuture;
use std::sync::Mutex;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::future_to_promise;

use super::{TransferableMessage, TransferableMessageType};

/// Takes a single requests and can return a stream of responses using the provided callback.
///
/// Its main use is integrating with synchronous code based on callbacks, for example to report progress.
pub trait CallbackWorker: WebWorker {
    /// Executes the worker implementation. Should not be called by user code, use an [Executor](crate::executors) instead.
    fn stream_callback(
        request: Self::Request,
        callback: Box<dyn Fn(Self::Response)>,
    ) -> LocalBoxFuture<'static, ()>;
}

#[wasm_bindgen]
#[derive(Clone)]
#[doc(hidden)]
pub struct CallbackWorkerFn {
    pub(crate) path: &'static str,
    pub(crate) function: fn(TransferableMessage, Box<dyn Fn(TransferableMessage)>),
}

impl CallbackWorkerFn {
    #[must_use]
    #[doc(hidden)]
    pub fn new<W: CallbackWorker>() -> Self {
        Self {
            path: W::path(),
            function: move |request, callback| {
                let callback = Rc::new(callback);
                let callback2 = callback.clone();
                let request_data = request.into_inner();
                let _ = future_to_promise(async move {
                    (W::stream_callback(
                        request_data,
                        Box::new(move |response| {
                            callback(TransferableMessage::new(
                                TransferableMessageType::Response,
                                response,
                            ));
                        }),
                    ))
                    .await;
                    callback2(TransferableMessage::new_null(
                        TransferableMessageType::Response,
                    ));
                    Ok(JsValue::undefined())
                });
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
            let mut opt = CALLBACK_WORKER_FN
                .lock()
                .expect("failed to lock CALLBACK_WORKER_FN");
            *opt = Some(callback_worker.clone());
        }
    }
}

pub(crate) static CALLBACK_WORKER_FN: Mutex<Option<CallbackWorkerFn>> = Mutex::new(None);
