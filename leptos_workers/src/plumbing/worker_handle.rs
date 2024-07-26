use crate::plumbing::{create_worker, CreateWorkerError};
use crate::workers::FutureWorker;
use crate::workers::StreamWorker;
use crate::workers::WebWorker;
use crate::workers::{CallbackWorker, TransferableMessage};
use crate::workers::{ChannelWorker, TransferableMessageType};
use alloc::rc::Rc;
use futures::{FutureExt, Stream, StreamExt};
use std::cell::RefCell;
use std::marker::PhantomData;
use tracing::warn;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{MessageEvent, Worker};

#[derive(Debug, Clone)]
pub struct WorkerHandle<W: WebWorker> {
    worker: Worker,
    _phantom: PhantomData<W>,
}

impl<W: WebWorker> WorkerHandle<W> {
    pub(crate) fn new() -> Result<Self, CreateWorkerError> {
        Ok(Self {
            worker: create_worker::<W>()?,
            _phantom: PhantomData,
        })
    }

    pub(crate) fn terminate(&self) {
        self.worker.terminate();
    }
}

impl<W: FutureWorker> WorkerHandle<W> {
    pub async fn run(&mut self, request: &W::Request) -> W::Response {
        let (tx, rx) = flume::bounded(1);

        let tx = Rc::new(RefCell::new(Some(tx)));
        let closure: Closure<dyn FnMut(MessageEvent)> = Closure::new(move |event: MessageEvent| {
            let response = TransferableMessage::decode(&event);
            let response_data = response.into_inner();
            let _ = tx
                .borrow_mut()
                .as_ref()
                .expect("failed to acquire mutable borrow")
                .send(response_data);
            tx.take();
        });
        {
            self.worker
                .set_onmessage(Some(closure.as_ref().unchecked_ref()));

            TransferableMessage::new(TransferableMessageType::ReqFuture, request)
                .post_to_worker(&self.worker);
        }

        rx.into_recv_async()
            .map(|r| r.expect("sender dropped before future resolved"))
            .await
    }
}

impl<W: StreamWorker> WorkerHandle<W> {
    pub fn stream(&mut self, request: &W::Request) -> impl Stream<Item = W::Response> {
        let (tx, rx) = flume::unbounded();

        let tx = Rc::new(RefCell::new(Some(tx)));
        let closure: Closure<dyn FnMut(MessageEvent)> = Closure::new(move |event: MessageEvent| {
            let response = TransferableMessage::decode(&event);
            if response.is_null() {
                tx.take();
            } else {
                let response_data = response.into_inner();
                if let Some(tx) = tx.borrow().as_ref() {
                    // this will error if the stream is dropped on the receiver side
                    // that's ok, however we have no choice but to keep going
                    let _ = tx.send(response_data);
                }
            }
        });
        {
            self.worker
                .set_onmessage(Some(closure.as_ref().unchecked_ref()));

            TransferableMessage::new(TransferableMessageType::ReqStream, request)
                .post_to_worker(&self.worker);
        }

        // this sentinel makes sure we drop the closure only after the stream is done
        let closure_sentinel =
            Box::pin(futures::stream::unfold(
                closure,
                |_closure| async move { None },
            ));
        rx.into_stream().chain(closure_sentinel)
    }
}

impl<W: CallbackWorker> WorkerHandle<W> {
    pub async fn stream_callback(
        &mut self,
        request: &W::Request,
        callback: impl Fn(W::Response) + 'static,
    ) {
        let (tx, rx) = flume::bounded::<()>(1);
        let closure: Closure<dyn FnMut(MessageEvent)> = Closure::new(move |event: MessageEvent| {
            let response = TransferableMessage::decode(&event);
            if response.is_null() {
                if let Err(e) = tx.send(()) {
                    warn!("Couldn't send data in stream_callback. Was the promise dropped? {e:?}");
                }
            } else {
                let response_data: W::Response = response.into_inner();
                callback(response_data);
            }
        });
        {
            self.worker
                .set_onmessage(Some(closure.into_js_value().as_ref().unchecked_ref()));

            TransferableMessage::new(TransferableMessageType::ReqCallback, request)
                .post_to_worker(&self.worker);
        }
        let _ = rx.into_recv_async().await;
    }
}

impl<W: ChannelWorker> WorkerHandle<W> {
    pub fn channel(
        &mut self,
        init: W::Init,
    ) -> (flume::Sender<W::Request>, flume::Receiver<W::Response>) {
        // Send the init data through directly:
        TransferableMessage::new(TransferableMessageType::ReqChannel, init)
            .post_to_worker(&self.worker);

        let (request_tx, request_rx) = flume::unbounded::<W::Request>();
        let (response_tx, response_rx) = flume::unbounded::<W::Response>();
        let response_tx = Rc::new(RefCell::new(Some(response_tx)));
        let closure: Closure<dyn FnMut(MessageEvent)> = Closure::new(move |event: MessageEvent| {
            let response = TransferableMessage::decode(&event);
            if response.is_null() {
                *response_tx.borrow_mut() = None;
            } else {
                let response_data: W::Response = response.into_inner();
                // this will error if the stream is dropped on the receiver side
                // that's ok, however we have no choice but to keep going
                if let Some(response_tx) = response_tx.borrow().as_ref() {
                    let _ = response_tx.send(response_data);
                }
            }
        });
        let worker = self.worker.clone();
        worker.set_onmessage(Some(closure.into_js_value().as_ref().unchecked_ref()));
        spawn_local(async move {
            while let Ok(request) = request_rx.recv_async().await {
                TransferableMessage::new(TransferableMessageType::ReqChannel, request)
                    .post_to_worker(&worker);
            }
            TransferableMessage::new_null(TransferableMessageType::ReqChannel)
                .post_to_worker(&worker);
        });
        (request_tx, response_rx)
    }
}
