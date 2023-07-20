use crate::workers::web_worker::WebWorkerPath;
use js_sys::Array;
use thiserror::Error;
use wasm_bindgen::JsValue;
use web_sys::{window, Blob, Url, Worker, WorkerOptions, WorkerType};

#[derive(Error, Debug)]
pub enum CreateWorkerError {
    #[error("No window object found")]
    NoWindow,
    #[error("Javascript error when getting window.href: {0:?}")]
    WindowHref(JsValue),
    #[error("Javascript error when building the WASM URL: {0:?}")]
    WasmUrl(JsValue),
    #[error("Javascript error when creating the blob URL: {0:?}")]
    BlobUrl(JsValue),
    #[error("Javascript error when creating the worker: {0:?}")]
    NewWorker(JsValue),
}

pub fn create_worker<W: WebWorkerPath>() -> Result<Worker, CreateWorkerError> {
    let output_name = option_env!("LEPTOS_OUTPUT_NAME").unwrap_or_default();
    let site_pkg_dir = option_env!("LEPTOS_SITE_PKG_DIR").unwrap_or_default();
    let worker_js_blob = {
        let path = format!("/{site_pkg_dir}/{output_name}");
        let base = window()
            .ok_or(CreateWorkerError::NoWindow)?
            .location()
            .href()
            .map_err(CreateWorkerError::WindowHref)?;
        let url = Url::new_with_base(&path, &base)
            .map_err(CreateWorkerError::WasmUrl)?
            .to_string();

        string_to_blob(&format!(
            r#"
                import init from "{url}.js";
                
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
                    let mod = await init("{url}.wasm");
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
        ))?
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

fn string_to_blob(s: &str) -> Result<Blob, CreateWorkerError> {
    let json_jsvalue = JsValue::from_str(s);
    #[allow(clippy::from_iter_instead_of_collect)]
    let json_jsvalue_array = Array::from_iter(std::iter::once(json_jsvalue));

    Blob::new_with_str_sequence(&json_jsvalue_array).map_err(CreateWorkerError::BlobUrl)
}
