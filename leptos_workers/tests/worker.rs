use leptos_workers_macro::worker;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct TestRequest;
#[derive(Clone, Serialize, Deserialize)]
pub struct TestResponse;

#[worker(TestCallbackWorker)]
pub async fn callback_worker(_req: TestRequest, _callback: impl Fn(TestResponse)) {}

#[worker(TestChannelWorker)]
pub async fn channel_worker(
    _rx: leptos_workers::Receiver<TestRequest>,
    _tx: leptos_workers::Sender<TestResponse>,
) {
}

#[worker(TestFutureWorker)]
pub async fn future_worker(_req: TestRequest) -> Vec<TestResponse> {
    vec![TestResponse]
}

#[worker(TestStreamWorker)]
pub async fn stream_worker(_req: TestRequest) -> impl leptos_workers::Stream<Item = TestResponse> {
    futures::stream::once(async { TestResponse })
}

#[test]
fn pass() {
    // this is where you test end-to-end, should actually create a pool etc
    // gonna involve a lot of wasm-bindgen
    //let _ = callback_worker(TestRequest, move |resp: TestResponse| {});
    //let _ = channel_worker();
    //let _ = future_worker(TestRequest);
    //let _ = stream_worker(&TestRequest);
}
