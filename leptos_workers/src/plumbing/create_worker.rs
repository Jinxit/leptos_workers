use crate::workers::WebWorkerPath;
use js_sys::Array;
use thiserror::Error;
use wasm_bindgen::JsValue;
use web_sys::{window, Blob, BlobPropertyBag, Url, Worker, WorkerOptions, WorkerType};

/// Describes failures related to worker creation.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum CreateWorkerError {
    #[doc(hidden)]
    #[error("No window object found")]
    NoWindow,
    #[doc(hidden)]
    #[error("Javascript error when getting window.href: {0:?}")]
    WindowHref(JsValue),
    #[doc(hidden)]
    #[error("Javascript error when building the WASM URL: {0:?}")]
    WasmUrl(JsValue),
    #[doc(hidden)]
    #[error("Javascript error when creating the blob URL: {0:?}")]
    BlobUrl(JsValue),
    /// Failure due to issues related to WASM packaging or browser compatibility.
    #[error("Javascript error when creating the worker: {0:?}")]
    NewWorker(JsValue),
}

pub fn create_worker<W: WebWorkerPath>() -> Result<Worker, CreateWorkerError> {
    let site_pkg_dir = option_env!("LEPTOS_SITE_PKG_DIR")
        .map(|s| format!("/{s}"))
        .unwrap_or_default();
    let output_name = option_env!("LEPTOS_OUTPUT_NAME");
    let worker_js_blob = {
        let (js_path, wasm_path) = if let Some(output_name) = output_name {
            let base = format!("{site_pkg_dir}/{output_name}");
            (format!("{base}.js"), format!("{base}.wasm"))
        } else {
            let crate_name = "wasm-bindgen-test";
            (format!("/{crate_name}"), format!("{crate_name}_bg.wasm"))
        };
        let base = window()
            .ok_or(CreateWorkerError::NoWindow)?
            .location()
            .href()
            .map_err(CreateWorkerError::WindowHref)?;
        let js_url = Url::new_with_base(&js_path, &base)
            .map_err(CreateWorkerError::WasmUrl)?
            .to_string();
        let wasm_url = Url::new_with_base(&wasm_path, &base)
            .map_err(CreateWorkerError::WasmUrl)?
            .to_string();

        string_to_blob(
            BlobPropertyBag::new().type_("application/javascript"),
            &format!(
                r#"
                import init from "{js_url}";
                
                let queue = [];
                self.onmessage = event => {{
                    queue.push(event);
                }};
                self.set_onmessage = (callback) => {{
                    self.onmessage = callback;
                    for (const event of queue) {{
                        self.onmessage(event);
                    }}
                    queue = [];
                }}
                
                async function load() {{
                    let mod = await init("{wasm_url}");
                    let {{ init_workers, register_future_worker, register_stream_worker, register_callback_worker, register_channel_worker, memory }} = mod;
                    
                    let future_worker_fn = mod["WORKERS_FUTURE_" + self.name];
                    if (future_worker_fn) {{
                        register_future_worker(mod["WORKERS_FUTURE_" + self.name]());
                    }}
                    
                    let stream_worker_fn = mod["WORKERS_STREAM_" + self.name];
                    if (stream_worker_fn) {{
                        register_stream_worker(mod["WORKERS_STREAM_" + self.name]());
                    }}
                    
                    let callback_worker_fn = mod["WORKERS_CALLBACK_" + self.name];
                    if (callback_worker_fn) {{
                        register_callback_worker(mod["WORKERS_CALLBACK_" + self.name]());
                    }}
                    
                    let channel_worker_fn = mod["WORKERS_CHANNEL_" + self.name];
                    if (channel_worker_fn) {{
                        register_channel_worker(mod["WORKERS_CHANNEL_" + self.name]());
                    }}
                    
                    init_workers();
                }}
                load();
            "#
            ),
        )?
    };
    let blob_url =
        Url::create_object_url_with_blob(&worker_js_blob).map_err(CreateWorkerError::BlobUrl)?;
    let worker = Worker::new_with_options(
        &blob_url,
        WorkerOptions::new()
            .name(W::path())
            .type_(WorkerType::Module),
    )
    .map_err(CreateWorkerError::NewWorker)?;

    Ok(worker)
}

fn string_to_blob(options: &BlobPropertyBag, s: &str) -> Result<Blob, CreateWorkerError> {
    let json_jsvalue = JsValue::from_str(s);
    #[allow(clippy::from_iter_instead_of_collect)]
    let json_jsvalue_array = Array::from_iter(std::iter::once(json_jsvalue));

    Blob::new_with_str_sequence_and_options(&json_jsvalue_array, options)
        .map_err(CreateWorkerError::BlobUrl)
}
