use crate::workers::web_worker::WebWorker;
use futures::future::LocalBoxFuture;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    cell::RefCell,
    sync::{atomic::AtomicBool, Arc, Mutex},
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

use super::{TransferableMessage, TransferableMessageType};

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

#[wasm_bindgen]
#[derive(Clone)]
#[doc(hidden)]
pub struct ChannelWorkerFn {
    pub(crate) path: &'static str,
    pub(crate) function: Arc<
        dyn Fn(TransferableMessage, Box<dyn Fn(TransferableMessage) + Send + Sync + 'static>)
            + 'static,
    >,
}

impl ChannelWorkerFn {
    #[must_use]
    #[doc(hidden)]
    pub fn new<W: ChannelWorker>() -> Self {
        /*
           Note: This callback starts empty and gets set each time a message is sent,
                 which is probably unnecessary - but it's the easiest way without storing
                 state after initialization
        */
        type Callback =
            Arc<Mutex<Option<Box<dyn Fn(TransferableMessage) + Send + Sync + 'static>>>>;
        let callback: Callback = Arc::default();
        let (request_tx, request_rx) = flume::unbounded();
        let (response_tx, response_rx) = flume::unbounded();
        let request_tx = Arc::new(Mutex::new(Some(request_tx)));

        let initializer = Mutex::new(Some(move |callback: &Callback, init: W::Init| {
            spawn_local(Box::pin(W::channel(init, request_rx, response_tx)));
            spawn_local(Box::pin({
                let callback = callback.clone();
                async move {
                    while let Ok(response) = response_rx.recv_async().await {
                        let callback = callback.lock().expect("mutex should not be poisoned");
                        if let Some(callback) = callback.as_ref() {
                            callback(TransferableMessage::new(
                                TransferableMessageType::Response,
                                response,
                            ));
                        }
                    }
                    let callback = callback.lock().expect("mutex should not be poisoned");
                    if let Some(callback) = callback.as_ref() {
                        callback(TransferableMessage::new_null(
                            TransferableMessageType::Response,
                        ));
                    }
                }
            }));
        }));

        let already_initialized = AtomicBool::new(false);
        Self {
            path: W::path(),
            function: Arc::new(move |request, new_callback| {
                if request.is_null() {
                    let mut request_tx = request_tx.lock().expect("mutex should not be poisoned");
                    *request_tx = None;
                } else if let Some(request_tx) =
                    &*request_tx.lock().expect("mutex should not be poisoned")
                {
                    *callback.lock().expect("mutex should not be poisoned") = Some(new_callback);

                    if already_initialized.swap(true, std::sync::atomic::Ordering::Relaxed) {
                        let request_data = request.into_inner();
                        // this will error when the receiver is dropped
                        let _ = request_tx.send(request_data);
                    } else {
                        // On first call, the message will contain the init data that can be used to start the worker fn.
                        (initializer
                            .lock()
                            .expect("mutex should not be poisoned")
                            .take()
                            .expect("Initializer shouldn't have been taken yet."))(
                            &callback,
                            request.into_inner(),
                        );
                    }
                }
            }),
        }
    }
}

mod private {

    use crate::workers::channel_worker::{ChannelWorkerFn, CHANNEL_WORKER_FN};
    use js_sys::global;
    use wasm_bindgen::prelude::wasm_bindgen;
    use wasm_bindgen::JsValue;
    use web_sys::DedicatedWorkerGlobalScope;

    #[wasm_bindgen]
    pub fn register_channel_worker(channel_worker: &ChannelWorkerFn) {
        console_error_panic_hook::set_once();

        let worker_scope: DedicatedWorkerGlobalScope = JsValue::from(global()).into();
        if worker_scope.name() == channel_worker.path {
            CHANNEL_WORKER_FN.with_borrow_mut(move |opt| {
                *opt = Some(channel_worker.clone());
            });
        }
    }
}

thread_local! {
    pub(crate) static CHANNEL_WORKER_FN: RefCell<Option<ChannelWorkerFn>> = const { RefCell::new(None) };
}
