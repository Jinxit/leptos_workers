use crate::workers::CHANNEL_WORKER_FN;
use crate::workers::FUTURE_WORKER_FN;
use crate::workers::STREAM_WORKER_FN;
use crate::workers::{TransferableMessage, TransferableMessageType, CALLBACK_WORKER_FN};
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

        let msg = TransferableMessage::decode(&event);
        match msg.message_type() {
            TransferableMessageType::ReqFuture => {
                spawn_local(async move {
                    on_message_future_worker(&worker_scope, msg).await;
                });
            }
            TransferableMessageType::ReqStream => {
                spawn_local(async move {
                    on_message_stream_worker(&worker_scope, msg).await;
                });
            }
            TransferableMessageType::ReqCallback => {
                on_message_callback_worker(worker_scope, msg);
            }
            TransferableMessageType::ReqChannel => {
                on_message_channel_worker(msg);
            }
            TransferableMessageType::Response => {
                panic!("Wasn't expecting a response here");
            }
        }
    });

    custom_worker_scope.set_onmessage(onmessage.as_ref().unchecked_ref());

    onmessage.forget();
}

fn on_message_channel_worker(msg: TransferableMessage) {
    CHANNEL_WORKER_FN.with_borrow(move |channel_worker_fn| {
        let channel = &channel_worker_fn
            .as_ref()
            .expect("Tried to use a ChannelWorker which was not registered.")
            .function;
        channel(
            msg,
            Box::new(move |response| {
                let worker_scope: DedicatedWorkerGlobalScope = JsValue::from(global()).into();
                response.post_from_worker(&worker_scope);
            }),
        );
    })
}

fn on_message_callback_worker(worker_scope: DedicatedWorkerGlobalScope, msg: TransferableMessage) {
    let callback_worker_fn = CALLBACK_WORKER_FN
        .lock()
        .expect("failed to lock mutex for CallbackWorker");
    let stream_callback = callback_worker_fn
        .as_ref()
        .expect("Tried to use a CallbackWorker which was not registered.")
        .function;

    stream_callback(
        msg,
        Box::new(move |response| {
            response.post_from_worker(&worker_scope);
        }),
    );
}

async fn on_message_stream_worker(
    worker_scope: &DedicatedWorkerGlobalScope,
    msg: TransferableMessage,
) {
    let mut stream = {
        let callback_worker_fn = STREAM_WORKER_FN
            .lock()
            .expect("failed to lock mutex for StreamWorker");
        let stream = callback_worker_fn
            .as_ref()
            .expect("Tried to use a StreamWorker which was not registered.")
            .function;

        stream(msg)
    };

    while let Some(response) = stream.next().await {
        response.post_from_worker(&worker_scope);
    }
    TransferableMessage::new_null(TransferableMessageType::Response)
        .post_from_worker(&worker_scope);
}

async fn on_message_future_worker(
    worker_scope: &DedicatedWorkerGlobalScope,
    msg: TransferableMessage,
) {
    let future = {
        let future_worker_fn = FUTURE_WORKER_FN
            .lock()
            .expect("failed to lock mutex for FutureWorker");
        let run = future_worker_fn
            .as_ref()
            .expect("Tried to use a FutureWorker which was not registered.")
            .function;

        run(msg)
    };

    let response = future.await;
    response.post_from_worker(&worker_scope);
}
