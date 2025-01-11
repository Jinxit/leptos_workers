use crate::{
    messages::{WorkerMsg, WorkerMsgType},
    workers::web_worker::WebWorker,
};
use futures::future::LocalBoxFuture;
use std::cell::RefCell;

/// Takes a single request and responds with a single response.
/// Technically, the implementation doesn't even have to be asynchronous - but when executed
/// it will still appear as such to the main thread.
///
/// Its main use is for running a single calculation without the need for progress updates.
pub trait FutureWorker: WebWorker {
    /// Executes the worker implementation. Should not be called by user code, use an [Executor](crate::executors) instead.
    fn run(request: Self::Request) -> LocalBoxFuture<'static, Self::Response>;
}

pub(crate) struct FutureWorkerFn {
    pub(crate) _path: &'static str,
    pub(crate) function: fn(WorkerMsg) -> LocalBoxFuture<'static, WorkerMsg>,
}

impl FutureWorkerFn {
    fn new<W: FutureWorker>() -> Self {
        Self {
            _path: W::path(),
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

#[doc(hidden)]
pub fn register_future_worker<W: FutureWorker>() {
    fn register(future_worker: FutureWorkerFn) {
        console_error_panic_hook::set_once();

        FUTURE_WORKER_FN.with_borrow_mut(move |opt| {
            *opt = Some(future_worker);
        });
    }
    register(FutureWorkerFn::new::<W>());
}

thread_local! {
    pub(crate) static FUTURE_WORKER_FN: RefCell<Option<FutureWorkerFn>> = const { RefCell::new(None) };
}
