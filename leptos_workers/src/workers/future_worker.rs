use crate::{
    worker_message::{WorkerMsg, WorkerMsgType},
    workers::web_worker::WebWorker,
};
use futures::future::LocalBoxFuture;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;

/// Takes a single request and responds with a single response.
/// Technically, the implementation doesn't even have to be asynchronous - but when executed
/// it will still appear as such to the main thread.
///
/// Its main use is for running a single calculation without the need for progress updates.
pub trait FutureWorker: WebWorker {
    /// Executes the worker implementation. Should not be called by user code, use an [Executor](crate::executors) instead.
    fn run(request: Self::Request) -> LocalBoxFuture<'static, Self::Response>;
}

#[wasm_bindgen]
#[derive(Clone)]
#[doc(hidden)]
pub struct FutureWorkerFn {
    pub(crate) path: &'static str,
    pub(crate) function: fn(WorkerMsg) -> LocalBoxFuture<'static, WorkerMsg>,
}

impl FutureWorkerFn {
    #[must_use]
    #[doc(hidden)]
    pub fn new<W: FutureWorker>() -> Self {
        Self {
            path: W::path(),
            function: move |request| {
                let request_data = request.into_inner();
                Box::pin(async move {
                    let response = W::run(request_data).await;
                    WorkerMsg::new(WorkerMsgType::Response, response)
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
            let mut opt = FUTURE_WORKER_FN
                .lock()
                .expect("failed to lock FUTURE_WORKER_FN");
            *opt = Some(future_worker.clone());
        }
    }
}

pub(crate) static FUTURE_WORKER_FN: Mutex<Option<FutureWorkerFn>> = Mutex::new(None);
