#![cfg(target_arch = "wasm32")]
use std::collections::HashMap;

use js_sys::Uint8Array;
use leptos_workers_macro::worker;
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, JsValue};
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
        let bag = web_sys::BlobPropertyBag::new();
        bag.set_type("image/jpeg");
        let image = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &bag).unwrap();
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
