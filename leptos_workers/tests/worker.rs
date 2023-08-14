use flume::TryRecvError;
use futures::StreamExt;
use leptos_workers_macro::worker;
use serde::{Deserialize, Serialize};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct TestRequest(i32);
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct TestResponse(i32);

#[worker(TestCallbackWorker)]
pub async fn callback_worker(req: TestRequest, callback: impl Fn(TestResponse)) {
    gloo_timers::future::TimeoutFuture::new(5000).await;
    callback(TestResponse(req.0 * 2));
}

#[worker(TestChannelWorker)]
pub async fn channel_worker(
    rx: leptos_workers::Receiver<TestRequest>,
    tx: leptos_workers::Sender<TestResponse>,
) {
    while let Ok(req) = rx.recv_async().await {
        tx.send_async(TestResponse(req.0 * 2)).await.unwrap();
    }
}

#[worker(TestFutureWorker)]
pub async fn future_worker(req: TestRequest) -> TestResponse {
    gloo_timers::future::TimeoutFuture::new(5000).await;
    TestResponse(req.0 * 2)
}

#[worker(TestStreamWorker)]
pub async fn stream_worker(req: TestRequest) -> impl leptos_workers::Stream<Item = TestResponse> {
    futures::stream::unfold(0, move |i| async move {
        match i {
            0 | 1 => {
                gloo_timers::future::TimeoutFuture::new(5000).await;
                Some((TestResponse(req.0 * (i + 2)), i + 1))
            }
            _ => None,
        }
    })
}

#[wasm_bindgen_test]
async fn callback_worker_test() {
    let callback = callback_worker(TestRequest(5), move |resp: TestResponse| {
        assert_eq!(resp.0, 10);
    });
    callback.await;
}

#[wasm_bindgen_test]
async fn channel_worker_test() {
    let (tx, rx) = channel_worker().await;
    tx.send_async(TestRequest(5)).await.unwrap();
    tx.send_async(TestRequest(7)).await.unwrap();
    let first = rx.recv_async().await.unwrap();
    let second = rx.recv_async().await.unwrap();
    assert_eq!(first.0, 10);
    assert_eq!(second.0, 14);
    std::mem::drop(tx);
    gloo_timers::future::TimeoutFuture::new(1000).await;
    let res = rx.try_recv();
    assert_eq!(res, Err(TryRecvError::Disconnected));
}

#[wasm_bindgen_test]
async fn future_worker_test() {
    let future = future_worker(TestRequest(5));
    let future_result = future.await;
    assert_eq!(future_result.0, 10);
}

#[wasm_bindgen_test]
async fn stream_worker_test() {
    let stream = stream_worker(&TestRequest(5));
    let stream_result: Vec<_> = stream.map(|r| r.0).collect().await;
    assert_eq!(stream_result, vec![10, 15]);
}
