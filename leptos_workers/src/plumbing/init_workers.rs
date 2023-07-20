use crate::plumbing::{WorkerRequest, WorkerRequestType};
use crate::workers::callback_worker::CALLBACK_WORKER_FN;
use crate::workers::channel_worker::CHANNEL_WORKER_FN;
use crate::workers::future_worker::FUTURE_WORKER_FN;
use crate::workers::stream_worker::STREAM_WORKER_FN;
use futures::StreamExt;
use js_sys::global;
use js_sys::Function;
use tracing_subscriber::fmt;
use tracing_subscriber_wasm::MakeConsoleWriter;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};

#[wasm_bindgen]
extern "C" {
    type CustomWorkerGlobalScope;

    #[wasm_bindgen(method)]
    pub fn set_onmessage(this: &CustomWorkerGlobalScope, value: &Function);
}

#[wasm_bindgen]
pub fn init_workers() {
    console_error_panic_hook::set_once();
    fmt()
        .with_writer(MakeConsoleWriter::default().map_trace_level_to(tracing::Level::DEBUG))
        // For some reason, if we don't do this in the browser, we get
        // a runtime error.
        .without_time()
        .with_ansi(false)
        .init();

    let worker_scope: DedicatedWorkerGlobalScope = JsValue::from(global()).into();

    let custom_worker_scope: CustomWorkerGlobalScope = worker_scope.unchecked_into();
    let onmessage: Closure<dyn FnMut(MessageEvent)> = Closure::new(move |event: MessageEvent| {
        let worker_scope: DedicatedWorkerGlobalScope = JsValue::from(global()).into();

        /*
            Note: There is a certain asymmetry going on here. Messages come in as a JsValue,
                  but at that point we do not know the type of the message. Is it a Future-,
                  a Callback- or a StreamWorker? That's why the message is serialized to a
                  Vec<u8> and then wrapped in a WorkerRequest.

                  This is mostly a remnant of the initial concept being structured around
                  a single worker being able to implement multiple of these traits, and could
                  potentially be refactored.
        */
        let request: WorkerRequest = serde_wasm_bindgen::from_value(event.data())
            .expect("failed to deserialize worker message");
        spawn_local(async move {
            match request.request_type {
                WorkerRequestType::Future => {
                    on_message_future_worker(&worker_scope, &request).await;
                }
                WorkerRequestType::Stream => {
                    on_message_stream_worker(&worker_scope, &request).await;
                }
                WorkerRequestType::Callback => {
                    on_message_callback_worker(worker_scope, &request).await;
                }
                WorkerRequestType::Channel => {
                    on_message_channel_worker(&request).await;
                }
            }
        });
    });

    custom_worker_scope.set_onmessage(onmessage.as_ref().unchecked_ref());

    onmessage.forget();
}

async fn on_message_channel_worker(request: &WorkerRequest) {
    let cell = CHANNEL_WORKER_FN
        .lock()
        .expect("failed to lock mutex for ChannelWorker");
    let channel_worker_fn = cell.borrow();
    let channel = &channel_worker_fn
        .as_ref()
        .expect("Tried to use a ChannelWorker which was not registered.")
        .function;

    channel(
        &request.request_data,
        Box::new(move |response| {
            let worker_scope: DedicatedWorkerGlobalScope =
                JsValue::from(global()).into();
            worker_scope
                .post_message(&response)
                .expect("failed to post message for ChannelWorker");
        }),
    );
}

async fn on_message_callback_worker(worker_scope: DedicatedWorkerGlobalScope, request: &WorkerRequest) {
    let cell = CALLBACK_WORKER_FN
        .lock()
        .expect("failed to lock mutex for CallbackWorker");
    let callback_worker_fn = cell.borrow();
    let stream_callback = callback_worker_fn
        .as_ref()
        .expect("Tried to use a CallbackWorker which was not registered.")
        .function;

    stream_callback(
        &request.request_data,
        Box::new(move |response| {
            worker_scope
                .post_message(&response)
                .expect("failed to post message for CallbackWorker");
        }),
    );
}

async fn on_message_stream_worker(worker_scope: &DedicatedWorkerGlobalScope, request: &WorkerRequest) {
    let mut stream = {
        let cell = STREAM_WORKER_FN
            .lock()
            .expect("failed to lock mutex for StreamWorker");
        let callback_worker_fn = cell.borrow();
        let stream = callback_worker_fn
            .as_ref()
            .expect("Tried to use a StreamWorker which was not registered.")
            .function;

        stream(&request.request_data)
    };

    while let Some(response) = stream.next().await {
        worker_scope
            .post_message(&response)
            .expect("failed to post message for StreamWorker");
    }
}

async fn on_message_future_worker(worker_scope: &DedicatedWorkerGlobalScope, request: &WorkerRequest) {
    let future = {
        let cell = FUTURE_WORKER_FN
            .lock()
            .expect("failed to lock mutex for FutureWorker");
        let future_worker_fn = cell.borrow();
        let run = future_worker_fn
            .as_ref()
            .expect("Tried to use a FutureWorker which was not registered.")
            .function;

        run(&request.request_data)
    };

    let response = future.await;
    worker_scope
        .post_message(&response)
        .expect("failed to post message for FutureWorker");
}
