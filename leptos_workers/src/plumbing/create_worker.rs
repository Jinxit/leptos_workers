use js_sys::Array;
use thiserror::Error;
use tracing::warn;
use wasm_bindgen::JsValue;
use web_sys::{window, Blob, BlobPropertyBag, Url, Worker, WorkerOptions, WorkerType};

use crate::workers::WebWorker;

/// Describes failures related to worker creation.
#[derive(Error, Debug, Clone)]
#[non_exhaustive]
pub enum CreateWorkerError {
    #[doc(hidden)]
    #[error("No window object found")]
    NoWindow,
    #[doc(hidden)]
    #[error("No document object found")]
    NoDocument,
    #[doc(hidden)]
    #[error("Javascript error when getting document.baseURI: {0:?}")]
    DocumentBaseUri(JsValue),
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

fn resolve_js_wasm_path(js_path: &str, wasm_path: &str) -> Result<(Url, Url), CreateWorkerError> {
    let document = window()
        .ok_or(CreateWorkerError::NoWindow)?
        .document()
        .ok_or(CreateWorkerError::NoDocument)?;
    let base_uri = document
        .base_uri()
        .map_err(CreateWorkerError::DocumentBaseUri)?;
    let create_uri = |url| match &base_uri {
        Some(base) => Url::new_with_base(url, base),
        None => Url::new(url),
    };
    let js_url = create_uri(js_path).map_err(CreateWorkerError::WasmUrl)?;
    let wasm_url = create_uri(wasm_path).map_err(CreateWorkerError::WasmUrl)?;
    Ok((js_url, wasm_url))
}

fn find_js_wasm_urls() -> Result<(Url, Url), CreateWorkerError> {
    // try to find the output name from the environment at compile time
    let output_name = option_env!("LEPTOS_OUTPUT_NAME").or_else(|| option_env!("CARGO_BIN_NAME"));

    // if we found the output name, use it as an additional condition just to be sure, otherwise skip it and hope for the best
    let output_name_condition = output_name
        .map(|s| format!("[href*='{s}']"))
        .unwrap_or_default();
    if output_name.is_none() {
        warn!("No output name found, if the worker is not loading, ensure either LEPTOS_OUTPUT_NAME or CARGO_BIN_NAME matches the output .wasm file name.");
    }

    // try to find the url from the <link> tags
    let document = window()
        .ok_or(CreateWorkerError::NoWindow)?
        .document()
        .ok_or(CreateWorkerError::NoDocument)?;
    let js_path = document
        .query_selector(&format!("head > link{output_name_condition}[href$='.js']"))
        .expect("query selector format to be valid")
        .map(|el| {
            el.get_attribute("href")
                .expect("query selector to only find <link> tags with href set")
        });
    let wasm_path = document
        .query_selector(&format!(
            "head > link{output_name_condition}[href$='.wasm']"
        ))
        .expect("query selector format to be valid")
        .map(|el| {
            el.get_attribute("href")
                .expect("query selector to only find <link> tags with href set")
        });

    let (js_path, wasm_path) = if let Some((js_path, wasm_path)) = js_path.zip(wasm_path) {
        (js_path, wasm_path)
    } else {
        let site_pkg_dir = option_env!("LEPTOS_SITE_PKG_DIR")
            .map(|s| format!("/{s}"))
            .unwrap_or_default();
        if let Some(output_name) = output_name {
            let base = format!("{site_pkg_dir}/{output_name}");
            (format!("{base}.js"), format!("{base}.wasm"))
        } else if let Some(output_name) = option_env!("CARGO_BIN_NAME") {
            (
                format!("{output_name}.js"),
                format!("{output_name}_bg.wasm"),
            )
        } else {
            // If we reached this point, we are likely in a test environment.
            // Worst case scenario, the user should see the earlier warning
            // about not finding the output name and act accordingly.
            (
                "wasm-bindgen-test.js".to_string(),
                "wasm-bindgen-test_bg.wasm".to_string(),
            )
        }
    };
    resolve_js_wasm_path(&js_path, &wasm_path)
}

pub fn create_unifunctional_worker<W: WebWorker>() -> Result<Worker, CreateWorkerError> {
    let (js_url, wasm_url) = find_js_wasm_urls()?;
    let module_def =
        super::unifunctional_worker::web_module(&js_url.to_string(), &wasm_url.to_string());
    create_worker_from_module(W::path(), &module_def)
}

fn create_worker_from_module(
    worker_name: &str,
    module_def: &str,
) -> Result<Worker, CreateWorkerError> {
    let worker_js_blob = string_to_blob(
        BlobPropertyBag::new().type_("application/javascript"),
        module_def,
    )?;
    let blob_url =
        Url::create_object_url_with_blob(&worker_js_blob).map_err(CreateWorkerError::BlobUrl)?;
    let worker = Worker::new_with_options(
        &blob_url,
        WorkerOptions::new()
            .name(worker_name)
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
