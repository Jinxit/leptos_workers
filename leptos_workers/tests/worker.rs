#![allow(clippy::self_assignment)]

use gloo_timers::future::TimeoutFuture;
use leptos_workers_macro::worker;
use serde::{Deserialize, Serialize};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct TestInit(i32);
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct TestRequest(i32);
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct TestResponse(i32);

#[worker(TestCallbackWorker)]
pub async fn callback_worker(req: TestRequest, callback: impl Fn(TestResponse)) {
    TimeoutFuture::new(5000).await;
    callback(TestResponse(req.0 * 2));
}

#[worker(TestCallbackWorkerMut)]
pub async fn callback_worker_with_mut_arg(mut req: TestRequest, callback: impl Fn(TestResponse)) {
    req = req;
    TimeoutFuture::new(5000).await;
    callback(TestResponse(req.0 * 2));
}

#[worker(TestChannelWorker)]
pub async fn channel_worker(
    init: TestInit,
    rx: leptos_workers::Receiver<TestRequest>,
    tx: leptos_workers::Sender<TestResponse>,
) {
    assert_eq!(init.0, 3);

    while let Ok(req) = rx.recv_async().await {
        tx.send_async(TestResponse(req.0 * 2)).await.unwrap();
    }
}

#[worker(TestChannelWorkerMut)]
pub async fn channel_worker_with_mut_arg(
    _init: TestInit,
    mut rx: leptos_workers::Receiver<TestRequest>,
    mut tx: leptos_workers::Sender<TestResponse>,
) {
    rx = rx;
    tx = tx;
    while let Ok(req) = rx.recv_async().await {
        tx.send_async(TestResponse(req.0 * 2)).await.unwrap();
    }
}

#[worker(TestFutureWorker)]
pub async fn future_worker(req: TestRequest) -> TestResponse {
    TimeoutFuture::new(5000).await;
    TestResponse(req.0 * 2)
}

#[worker(TestFutureWorkerMut)]
pub async fn future_worker_with_mut_arg(mut req: TestRequest) -> TestResponse {
    req = req;
    TimeoutFuture::new(5000).await;
    TestResponse(req.0 * 2)
}

#[worker(HeavyLoop)]
pub async fn heavy_loop(req: TestRequest) -> TestResponse {
    // this takes like 2 seconds on my pc, slow enough to be obvious
    for i in 0..10_000_000 {
        std::hint::black_box(i);
    }
    TestResponse(req.0 * 2)
}

#[worker(TestStreamWorker)]
pub async fn stream_worker(req: TestRequest) -> impl leptos_workers::Stream<Item = TestResponse> {
    futures::stream::unfold(0, move |i| async move {
        match i {
            0 | 1 => {
                TimeoutFuture::new(5000).await;
                Some((TestResponse(req.0 * (i + 2)), i + 1))
            }
            _ => None,
        }
    })
}

#[worker(TestStreamWorkerMut)]
pub async fn stream_worker_with_mut_arg(
    mut req: TestRequest,
) -> impl leptos_workers::Stream<Item = TestResponse> {
    req = req;
    futures::stream::unfold(0, move |i| async move {
        match i {
            0 | 1 => {
                TimeoutFuture::new(5000).await;
                Some((TestResponse(req.0 * (i + 2)), i + 1))
            }
            _ => None,
        }
    })
}

#[cfg(not(feature = "ssr"))]
#[wasm_bindgen_test]
async fn callback_worker_test() {
    let callback = callback_worker(TestRequest(5), move |resp: TestResponse| {
        assert_eq!(resp.0, 10);
    });
    callback.await.unwrap();
}

#[cfg(not(feature = "ssr"))]
#[wasm_bindgen_test]
async fn channel_worker_test() {
    use flume::TryRecvError;
    use std::mem::drop;

    let (tx, rx) = channel_worker(TestInit(3)).unwrap();
    tx.send_async(TestRequest(5)).await.unwrap();
    tx.send_async(TestRequest(7)).await.unwrap();
    let first = rx.recv_async().await.unwrap();
    let second = rx.recv_async().await.unwrap();
    assert_eq!(first.0, 10);
    assert_eq!(second.0, 14);
    drop(tx);
    TimeoutFuture::new(1000).await;
    let res = rx.try_recv();
    assert_eq!(res, Err(TryRecvError::Disconnected));
}

#[cfg(not(feature = "ssr"))]
#[wasm_bindgen_test]
async fn future_worker_test() {
    let future = future_worker(TestRequest(5));
    let future_result = future.await;
    assert_eq!(future_result.unwrap().0, 10);
}

#[cfg(not(feature = "ssr"))]
#[wasm_bindgen_test]
async fn stream_worker_test() {
    use futures::StreamExt;

    let stream = stream_worker(&TestRequest(5));
    let stream_result: Vec<_> = stream.unwrap().map(|r| r.0).collect().await;
    assert_eq!(stream_result, vec![10, 15]);
}

#[cfg(not(feature = "ssr"))]
#[wasm_bindgen_test]
async fn heavy_loop_is_interactive() {
    use std::{
        borrow::BorrowMut,
        sync::{Arc, Mutex},
    };
    use web_sys::window;

    let done = Arc::new(Mutex::new(false));

    let test = {
        let done = done.clone();
        move || {
            let done = done.clone();
            async move {
                let future = heavy_loop(TestRequest(5));
                let future_result = future.await;
                assert_eq!(future_result.unwrap().0, 10);
                **done.lock().unwrap().borrow_mut() = true;
            }
        }
    };

    let timer = async move {
        let window = window().expect("should have a window in this context");
        let performance = window
            .performance()
            .expect("performance should be available");

        let mut counter = 0;
        let mut total_diff = 0.0;
        let mut last_tick = performance.now();
        while !*done.lock().unwrap() {
            TimeoutFuture::new(0).await;
            let now = performance.now();
            // 100 ms is enough to not have flaky tests but still plenty below the expected 2 seconds
            // if this was synchronous. ideally it would be below 16 ms (60 fps) for true interactivity
            // but i think i'm running into the typical microbenchmark issues with such a low limit
            let diff = now - last_tick;
            assert!(diff < 100.0, "{diff} ms is too slow!");
            counter += 1;
            total_diff += diff;
            last_tick = now;
        }
        // but we CAN say the total average should be below 16 ms
        let average_diff = total_diff / counter as f64;
        assert!(
            average_diff < 1000.0 / 60.0,
            "{average_diff} ms average is too slow!"
        );
    };

    futures::join!(timer, test(), test(), test());
}

#[test]
#[should_panic]
fn should_panic_on_ssr() {
    pollster::block_on(async { future_worker(TestRequest(5)).await.unwrap() });
}
