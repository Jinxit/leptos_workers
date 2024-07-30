#![allow(clippy::self_assignment)]

use std::collections::HashMap;

use futures::future::join_all;
use gloo_timers::future::TimeoutFuture;
use js_sys::Uint8Array;
use leptos_workers_macro::worker;
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, JsValue};
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

/// This one uses the init parameter.
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

/// This one has init param omitted.
#[worker(TestChannelWorkerNoInit)]
pub async fn channel_worker_no_init(
    rx: leptos_workers::Receiver<TestRequest>,
    tx: leptos_workers::Sender<TestResponse>,
) {
    while let Ok(req) = rx.recv_async().await {
        tx.send_async(TestResponse(req.0 * 2)).await.unwrap();
    }
}

/// Mutable args shouldn't break channel workers.
#[worker(TestChannelWorkerMut)]
pub async fn channel_worker_with_mut_arg(
    mut init: TestInit,
    mut rx: leptos_workers::Receiver<TestRequest>,
    mut tx: leptos_workers::Sender<TestResponse>,
) {
    init.0 = 4;
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

    // Check version with no init:
    let (tx, rx) = channel_worker_no_init().unwrap();
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

/// Seeded simple random byte generator fns.
fn seeded_rand_byte() -> impl FnMut() -> u8 {
    let mut next = 0;
    move || {
        next = (next + 1) % 255;
        next
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct TransMsgArrWithCheck(
    #[serde(with = "leptos_workers::transferable")] js_sys::Uint8Array,
    Vec<u8>,
);

impl TransMsgArrWithCheck {
    pub fn check(&self) {
        assert_eq!(self.0.length(), self.1.len() as u32);
        for (i, byte) in self.1.iter().enumerate() {
            assert_eq!(self.0.get_index(i as u32), *byte);
        }
    }
}

#[worker]
async fn worker_contended(req: TransMsgArrWithCheck) -> TransMsgArrWithCheck {
    req.check();
    req
}

#[wasm_bindgen_test]
async fn test_transferable_whilst_contended() {
    // Will concurrently call the worker this many times with varying requests:
    let num_calls = 3;

    let mut get_next_byte = seeded_rand_byte();
    let mut reqs = Vec::with_capacity(num_calls);
    for _ in 0..num_calls {
        let mut vec = Vec::with_capacity(10);
        for _ in 0..10 {
            vec.push(get_next_byte());
        }
        let js_arr = js_sys::Uint8Array::new_with_length(10);
        js_arr.copy_from(&vec);
        reqs.push(TransMsgArrWithCheck(js_arr, vec));
    }
    // Process all via join simulating concurrency/contended serialization/deserialization:
    for result in join_all(reqs.into_iter().map(worker_contended)).await {
        result.unwrap().check();
    }
}

/// Standard worker test, passing an object containing transferables to a worker and back again.
macro_rules! transferable_test {
    ($id:ident, $typ:ty, $create:block, $verify:expr) => {
        paste::item! {
            #[allow(non_camel_case_types)]
            #[derive(Clone, Serialize, Deserialize)]
            struct [< TransMsg_ $id >](#[serde(with = "leptos_workers::transferable")] $typ);

            #[worker]
            async fn [< worker_fn_ $id >](req: [< TransMsg_ $id >]) -> [< TransMsg_ $id >] {
                ($verify)(req.0.clone());
                req
            }

            #[wasm_bindgen_test]
            async fn [< test_transferable_ $id >]() {
                let contents = $create;
                ($verify)(contents.clone());
                let response_msg = [< worker_fn_ $id >]([< TransMsg_ $id >](contents))
                    .await
                    .unwrap();
                ($verify)(response_msg.0);
            }
        }
    };
}

transferable_test! {
    complex,
    HashMap<String, Vec<HashMap<String, Option<js_sys::Uint8Array>>>>,
    {
        let mut get_next_byte = seeded_rand_byte();
        let mut map = HashMap::new();
        for x in 0..10 {
            let mut vec = Vec::new();
            for _y in 0..10 {
                let mut inner_map = HashMap::new();
                for z in 0..10 {
                    let uint8_array = js_sys::Uint8Array::new(&js_sys::ArrayBuffer::new(10));
                    for w in 0..10 {
                        uint8_array.set_index(w as u32, get_next_byte());
                    }
                    inner_map.insert(z.to_string(), Some(uint8_array));
                }
                vec.push(inner_map);
            }
            map.insert(x.to_string(), vec);
        }
        map
    },
    |map: HashMap<String, Vec<HashMap<String, Option<js_sys::Uint8Array>>>>| {
        let mut get_next_byte = seeded_rand_byte();

        assert_eq!(map.len(), 10);
        for x in 0..10 {
            let vec = map.get(&x.to_string()).unwrap();
            assert_eq!(vec.len(), 10);
            for inner_map in vec.iter() {
                assert_eq!(inner_map.len(), 10);
                for z in 0..10 {
                    let uint8_array = inner_map.get(&z.to_string()).unwrap().as_ref().unwrap();
                    assert_eq!(uint8_array.length(), 10);
                    for w in 0..10 {
                        assert_eq!(uint8_array.get_index(w as u32), get_next_byte());
                    }
                }
            }
        }
    }
}

transferable_test! {
    uint8_array,
    js_sys::Uint8Array,
    {
        let uint8_array = js_sys::Uint8Array::new(&js_sys::ArrayBuffer::new(10));
        for x in 0..10 {
            uint8_array.set_index(x as u32, x);
        }
        uint8_array
    },
    |arr: js_sys::Uint8Array| {
        assert_eq!(arr.length(), 10);
        for x in 0..10 {
            assert_eq!(arr.get_index(x as u32), x);
        }
    }
}

transferable_test! {
    arb_js,
    JsValue,
    {
        let js_val: JsValue = js_sys::ArrayBuffer::new(10).into();
        js_val
    },
    |val: JsValue| {
        let arr = val.dyn_into::<js_sys::ArrayBuffer>().unwrap();
        assert_eq!(arr.byte_length(), 10);
    }
}

transferable_test! {
    option_t_some,
    Option<js_sys::ArrayBuffer>,
    {
        Some(js_sys::ArrayBuffer::new(10))
    },
    |opt: Option<js_sys::ArrayBuffer>| {
        let arr = opt.unwrap();
        assert_eq!(arr.byte_length(), 10);
    }
}

transferable_test! {
    option_t_none,
    Option<js_sys::ArrayBuffer>,
    {
        None
    },
    |opt: Option<js_sys::ArrayBuffer>| {
        assert!(opt.is_none());
    }
}

transferable_test! {
    vec_of_t,
    Vec<js_sys::Uint8Array>,
    {
        // 10 items, each arr with 10 elements, all ordered by index:
        let mut vec = Vec::new();
        for x in 0..10 {
            let uint8_array = js_sys::Uint8Array::new(&js_sys::ArrayBuffer::new(10));
            for y in 0..10 {
                uint8_array.set_index(y as u32, x * 10 + y);
            }
            vec.push(uint8_array);
        }
        vec
    },
    |vec: Vec<js_sys::Uint8Array>| {
        assert_eq!(vec.len(), 10);
        for (x, arr) in vec.into_iter().enumerate() {
            assert_eq!(arr.length(), 10);
            for y in 0..10 {
                assert_eq!(arr.get_index(y as u32), (x * 10 + y) as u8);
            }
        }
    }
}

transferable_test! {
    hashmap,
    HashMap<String, js_sys::Uint8Array>,
    {
        let mut map = HashMap::new();
        for x in 0..10 {
            let uint8_array = js_sys::Uint8Array::new(&js_sys::ArrayBuffer::new(10));
            for y in 0..10 {
                uint8_array.set_index(y as u32, x * 10 + y);
            }
            map.insert(x.to_string(), uint8_array);
        }
        map
    },
    |map: HashMap<String, js_sys::Uint8Array>| {
        assert_eq!(map.len(), 10);
        for (x, arr) in map.into_iter() {
            assert_eq!(arr.length(), 10);
            for y in 0..10 {
                assert_eq!(arr.get_index(y as u32), (x.parse::<u8>().unwrap() * 10 + y));
            }
        }
    }
}

// All the base-level types:
transferable_test! {
    array_buffer,
    js_sys::ArrayBuffer,
    {
        js_sys::ArrayBuffer::new(10)
    },
    |arr: js_sys::ArrayBuffer| {
        assert_eq!(arr.byte_length(), 10);
    }
}
transferable_test! {
    message_port,
    web_sys::MessagePort,
    {
        let channel = web_sys::MessageChannel::new().unwrap();
        channel.port1()
    },
    |port: web_sys::MessagePort| {
        assert!(port.is_instance_of::<web_sys::MessagePort>());
    }
}
transferable_test! {
    readable_stream,
    web_sys::ReadableStream,
    {
        web_sys::Blob::new().unwrap().stream()
    },
    |stream: web_sys::ReadableStream| {
        assert!(stream.is_instance_of::<web_sys::ReadableStream>());
    }
}
transferable_test! {
    writable_stream,
    web_sys::WritableStream,
    {
        web_sys::WritableStream::new().unwrap()
    },
    |stream: web_sys::WritableStream| {
        assert!(stream.is_instance_of::<web_sys::WritableStream>());
    }
}
transferable_test! {
    transform_stream,
    web_sys::TransformStream,
    {
        web_sys::TransformStream::new().unwrap()
    },
    |stream: web_sys::TransformStream| {
        assert!(stream.is_instance_of::<web_sys::TransformStream>());
    }
}
transferable_test! {
    image_bitmap,
    web_sys::ImageBitmap,
    {
        let jpg_data = include_bytes!("./assets/test_image.jpg");
        let arr = Uint8Array::new_with_length(jpg_data.len() as u32);
        arr.copy_from(jpg_data.as_slice());
        let parts = js_sys::Array::new();
        parts.push(&arr.buffer());
        let image = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, web_sys::BlobPropertyBag::new().type_("image/jpeg")).unwrap();
        wasm_bindgen_futures::JsFuture::from(
            web_sys::window().unwrap().create_image_bitmap_with_blob(&image).unwrap()
        ).await.unwrap().unchecked_into::<web_sys::ImageBitmap>()
    },
    |bitmap: web_sys::ImageBitmap| {
        assert!(bitmap.is_instance_of::<web_sys::ImageBitmap>());
    }
}
transferable_test! {
    offscreen_canvas,
    web_sys::OffscreenCanvas,
    {
        web_sys::OffscreenCanvas::new(100, 100).unwrap()
    },
    |canvas: web_sys::OffscreenCanvas| {
        assert!(canvas.is_instance_of::<web_sys::OffscreenCanvas>());
    }
}

// Only actually supported by safari currently:
// https://developer.mozilla.org/en-US/docs/Web/API/RTCDataChannel#browser_compatibility
// transferable_test! {
//     rtc_data_channel,
//     web_sys::RtcDataChannel,
//     {
//         web_sys::RtcPeerConnection::new().unwrap().create_data_channel("test")
//     },
//     |channel: web_sys::RtcDataChannel| {
//         assert!(channel.is_instance_of::<web_sys::RtcDataChannel>());
//     }
// }
