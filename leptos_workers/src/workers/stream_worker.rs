use crate::messages::{WorkerMsg, WorkerMsgType};
use crate::workers::web_worker::WebWorker;
use futures::stream::LocalBoxStream;
use futures::StreamExt;
use std::cell::RefCell;

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

pub(crate) struct StreamWorkerFn {
    pub(crate) _path: &'static str,
    pub(crate) function: fn(WorkerMsg) -> LocalBoxStream<'static, WorkerMsg>,
}

impl StreamWorkerFn {
    fn new<W: StreamWorker>() -> Self {
        Self {
            _path: W::path(),
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

#[doc(hidden)]
pub fn register_stream_worker<W: StreamWorker>() {
    fn register(stream_worker: StreamWorkerFn) {
        console_error_panic_hook::set_once();

        STREAM_WORKER_FN.with_borrow_mut(move |opt| {
            *opt = Some(stream_worker);
        });
    }
    register(StreamWorkerFn::new::<W>());
}

thread_local! {
    pub(crate) static STREAM_WORKER_FN: RefCell<Option<StreamWorkerFn>> = const { RefCell::new(None) };
}
