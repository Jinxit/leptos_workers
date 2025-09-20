use crate::{
    messages::{WorkerMsg, WorkerMsgType},
    workers::web_worker::WebWorker,
};
use futures::future::LocalBoxFuture;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::atomic::Ordering;
use std::{
    cell::RefCell,
    sync::{atomic::AtomicBool, Arc, Mutex},
};
use wasm_bindgen_futures::spawn_local;

/// Takes multiple requests and can reply with multiple responses using channels.
///
/// Its main use is to have a duplex connection to a persistent stateful worker, rather than one-off tasks.
pub trait ChannelWorker: WebWorker {
    /// The init data given to the worker.
    type Init: Clone + Serialize + DeserializeOwned;

    /// Executes the worker implementation. Should not be called by user code, use an [Executor](crate::executors) instead.
    fn channel(
        init: Self::Init,
        rx: flume::Receiver<Self::Request>,
        tx: flume::Sender<Self::Response>,
    ) -> LocalBoxFuture<'static, ()>;
}

pub(crate) struct ChannelWorkerFn {
    pub(crate) _path: &'static str,
    pub(crate) function:
        Arc<dyn Fn(WorkerMsg, Box<dyn Fn(WorkerMsg) + Send + Sync + 'static>) + 'static>,
}

impl ChannelWorkerFn {
    fn new<W: ChannelWorker>() -> Self {
        /*
           Note: This callback starts empty and gets set each time a message is sent,
                 which is probably unnecessary - but it's the easiest way without storing
                 state after initialization
        */
        type Callback = Arc<Mutex<Option<Box<dyn Fn(WorkerMsg) + Send + Sync + 'static>>>>;
        let callback: Callback = Arc::default();
        let request_tx: Arc<Mutex<Option<flume::Sender<W::Request>>>> = Arc::new(Mutex::new(None));

        let initializer = Mutex::new(Some(move |callback: &Callback, init: W::Init| {
            let (response_tx, response_rx) = flume::bounded(0);
            let (new_request_tx, request_rx) = flume::unbounded();
            spawn_local(Box::pin(W::channel(init, request_rx, response_tx)));
            spawn_local(Box::pin({
                let callback = callback.clone();
                async move {
                    while let Ok(response) = response_rx.recv_async().await {
                        let callback = callback.lock().expect("mutex should not be poisoned");
                        if let Some(callback) = callback.as_ref() {
                            callback(WorkerMsg::new(WorkerMsgType::Response, response));
                        }
                    }
                    let callback = callback.lock().expect("mutex should not be poisoned");
                    if let Some(callback) = callback.as_ref() {
                        callback(WorkerMsg::new_null(WorkerMsgType::Response));
                    }
                }
            }));
            new_request_tx
        }));

        let already_initialized = AtomicBool::new(false);
        Self {
            _path: W::path(),
            function: Arc::new(move |request, new_callback| {
                if request.is_null() {
                    let mut request_tx = request_tx.lock().expect("mutex should not be poisoned");
                    *request_tx = None;
                    already_initialized.store(false, Ordering::Relaxed);
                } else {
                    *callback.lock().expect("mutex should not be poisoned") = Some(new_callback);

                    if already_initialized.swap(true, Ordering::Relaxed) {
                        let request_data = request.into_inner();
                        // this will error when the receiver is dropped
                        if let Some(request_tx) =
                            &*request_tx.lock().expect("mutex should not be poisoned")
                        {
                            let _ = request_tx.send(request_data);
                        }
                    } else {
                        // On first call, the message will contain the init data that can be used to start the worker fn.
                        let lock = initializer.lock().expect("mutex should not be poisoned");
                        let initializer = lock.expect("Initializer shouldn't have been taken yet.");
                        let new_request_tx = initializer(&callback, request.into_inner());
                        *request_tx.lock().expect("mutex should not be poisoned") =
                            Some(new_request_tx);
                    }
                }
            }),
        }
    }
}

#[doc(hidden)]
pub fn register_channel_worker<W: ChannelWorker>() {
    fn register(channel_worker: ChannelWorkerFn) {
        console_error_panic_hook::set_once();

        CHANNEL_WORKER_FN.with_borrow_mut(move |opt| {
            *opt = Some(channel_worker);
        });
    }
    register(ChannelWorkerFn::new::<W>());
}

thread_local! {
    pub(crate) static CHANNEL_WORKER_FN: RefCell<Option<ChannelWorkerFn>> = const { RefCell::new(None) };
}
