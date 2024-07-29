use crate::messages::{WorkerMsg, WorkerMsgType};
use crate::workers::web_worker::WebWorker;
use futures::stream::LocalBoxStream;
use futures::StreamExt;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;

/// Takes a single request but can reply with multiple responses.
///
/// Its main usage is for running an asynchronous calculation with multiple responses, for example a
/// computation with an initial decent result which improves as the computation continues.
///
/// Workers should be created using the [`#[worker]`](crate::worker#stream) attribute macro.
pub trait StreamWorker: WebWorker {
    /// Executes the worker implementation. Should not be called by user code, use an [Executor](crate::executors) instead.
    fn stream(request: Self::Request) -> LocalBoxStream<'static, Self::Response>;
}

#[wasm_bindgen]
#[derive(Clone)]
#[doc(hidden)]
pub struct StreamWorkerFn {
    pub(crate) path: &'static str,
    pub(crate) function: fn(WorkerMsg) -> LocalBoxStream<'static, WorkerMsg>,
}

impl StreamWorkerFn {
    #[must_use]
    #[doc(hidden)]
    pub fn new<W: StreamWorker>() -> Self {
        Self {
            path: W::path(),
            function: move |request| {
                let request_data = request.into_inner();
                Box::pin(
                    W::stream(request_data)
                        .map(|response| WorkerMsg::new(WorkerMsgType::Response, response)),
                )
            },
        }
    }
}

mod private {
    use crate::workers::stream_worker::{StreamWorkerFn, STREAM_WORKER_FN};
    use js_sys::global;
    use wasm_bindgen::prelude::wasm_bindgen;
    use wasm_bindgen::JsValue;
    use web_sys::DedicatedWorkerGlobalScope;

    #[wasm_bindgen]
    pub fn register_stream_worker(stream_worker: &StreamWorkerFn) {
        console_error_panic_hook::set_once();

        let worker_scope: DedicatedWorkerGlobalScope = JsValue::from(global()).into();
        if worker_scope.name() == stream_worker.path {
            let mut opt = STREAM_WORKER_FN
                .lock()
                .expect("failed to lock STREAM_WORKER_FN");
            *opt = Some(stream_worker.clone());
        }
    }
}

pub(crate) static STREAM_WORKER_FN: Mutex<Option<StreamWorkerFn>> = Mutex::new(None);
