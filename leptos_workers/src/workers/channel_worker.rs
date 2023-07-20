use crate::codec;
use crate::workers::web_worker::WebWorker;
use futures::future::BoxFuture;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

pub trait ChannelWorker: WebWorker {
    fn channel(
        tx: flume::Sender<Self::Response>,
        rx: flume::Receiver<Self::Request>,
    ) -> BoxFuture<'static, ()>;
}

#[wasm_bindgen(inspectable)]
#[derive(Clone)]
pub struct ChannelWorkerFn {
    pub(crate) path: &'static str,
    pub(crate) function:
        Arc<dyn Fn(&Vec<u8>, Box<dyn Fn(JsValue) + Send + Sync + 'static>) + Send + Sync + 'static>,
}

impl ChannelWorkerFn {
    #[must_use]
    pub fn new<W: ChannelWorker>() -> Self {
        /*
           Note: This callback starts empty and gets set each time a message is sent,
                 which is probably unnecessary - but it's the easiest way without storing
                 state after initialization
        */
        let callback: Arc<Mutex<Option<Box<dyn Fn(JsValue) + Send + Sync + 'static>>>> =
            Arc::default();
        let (request_tx, request_rx) = flume::unbounded();
        let (response_tx, response_rx) = flume::unbounded();
        spawn_local(Box::pin(W::channel(response_tx, request_rx)));
        spawn_local(Box::pin({
            let callback = callback.clone();
            async move {
                while let Ok(response) = response_rx.recv_async().await {
                    let value =
                        serde_wasm_bindgen::to_value(&response).expect("js serialization error");
                    let callback = callback.lock().expect("mutex should not be poisoned");
                    if let Some(callback) = callback.as_ref() {
                        callback(value);
                    }
                }
                let callback = callback.lock().expect("mutex should not be poisoned");
                if let Some(callback) = callback.as_ref() {
                    callback(JsValue::NULL);
                }
            }
        }));
        Self {
            path: W::path(),
            function: Arc::new(move |request, new_callback| {
                let request = codec::from_slice(&request[..]).expect("byte deserialization error");
                let mut callback = callback.lock().expect("mutex should not be poisoned");
                *callback = Some(new_callback);
                // this will error when the receiver is dropped
                let _ = request_tx.send(request);
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
            let cell = CHANNEL_WORKER_FN
                .lock()
                .expect("failed to lock STREAM_WORKER_FN");
            let mut opt = cell.borrow_mut();
            *opt = Some(channel_worker.clone());
        }
    }
}

pub(crate) static CHANNEL_WORKER_FN: Mutex<RefCell<Option<ChannelWorkerFn>>> =
    Mutex::new(RefCell::new(None));
