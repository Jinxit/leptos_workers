#![cfg(target_arch = "wasm32")]
/// NOTE: this test doesn't do much until underlying worker contention issue fixed.
/// Once fixed, increasing num_calls = x will "properly" enable this test.
/// At the moment it would error after an increase with a generic worker issue.
use futures::future::join_all;
use leptos_workers_macro::worker;
use serde::{Deserialize, Serialize};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

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
