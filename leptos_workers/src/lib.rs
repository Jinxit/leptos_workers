#![warn(clippy::pedantic)]
#![allow(clippy::manual_async_fn)]
#![allow(clippy::type_complexity)]
#![allow(clippy::module_name_repetitions)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::panic)]
#![warn(missing_docs)]
#![allow(clippy::doc_markdown)]

//! This is an abstraction layer on top of Web Workers to make them easier to work with.
//! Web workers are useful to offload heavy computations from the main rendering thread,
//! keeping the user interface interactive even if the computation is an intense synchronous loop.
//!
//! To get started, take a look at the documentation for the [worker] macro.
//!
//! **Note**: The crate relies on having support for
//! [ECMAScript modules in workers](https://caniuse.com/mdn-api_worker_worker_ecmascript_modules),
//! so keep that in mind with regards to compatibility.
//!
//! For support, check out the #libraries channel on the official [Leptos Discord Channel](https://discord.gg/v38Eef6sWG).
//! Feel free to ping @Jinxit.

mod codec;
pub mod executors;
mod plumbing;
pub mod workers;

/// The main macro powering worker functions.
///
/// This macro accepts the following arguments:
///
/// 1. **Required**: The name of the worker, typically in PascalCase.
///    This will not be visible if using the worker function directly, but is used internally
///    as well as if you decide to use explicit executors.
///
/// Worker functions come in four flavours. Common to all are the following requirements:
///
/// - The Request and Response types must implement `Clone`, `Send`, [`Serialize`](https://docs.rs/serde/latest/serde/trait.Serialize.html)
///   and [`DeserializeOwned`](https://docs.rs/serde/latest/serde/de/trait.DeserializeOwned.html).
///
/// The different flavours all have slightly different usage scenarios:
///
/// | Flavour | Request | Response | Usecase |
/// | - | - | - | - |
/// | **[Callback](attr.worker.html#callback)** | Single | Multiple | Integrating with synchronous code based on callbacks, for example to report progress |
/// | **[Channel](attr.worker.html#channel)** | Multiple | Multiple | For a duplex connection to a persistent worker, rather than one-off tasks |
/// | **[Future](attr.worker.html#future)** | Single | Single | Running a single calculation without the need for progress updates |
/// | **[Stream](attr.worker.html#stream)** | Single | Multiple | Running an asynchronous calculation with multiple responses |
///
/// ## Callback
/// See also: [`CallbackWorker`](crate::workers::CallbackWorker).
///
/// ```ignore
///     [pub] [async] fn worker(
///         req: Request,
///         callback: impl Fn(Response)
///     )
/// ```
///
/// ## Channel
/// See also: [`ChannelWorker`](crate::workers::ChannelWorker).
///
/// ```ignore
///     [pub] [async] fn worker(
///         rx: leptos_workers::Receiver<Request>,
///         tx: leptos_workers::Sender<Response>
///     )
/// ```
///
/// The signature for using channel workers will be inverted to:
///
/// ```ignore
///     [pub] async fn worker() -> (
///         leptos_workers::Sender<Request>,
///         leptos_workers::Receiver<Response>
///     )
/// ```
///
/// ## Future
/// See also: [`FutureWorker`](crate::workers::FutureWorker).
///
/// ```ignore
///     [pub] [async] fn worker(
///         req: Request
///     ) -> Response
/// ```
///
/// ## Stream
/// See also: [`StreamWorker`](crate::workers::StreamWorker).
///
/// ```ignore
///     [pub] [async] fn worker(
///         req: Request
///     ) -> impl leptos_workers::Stream<Item = Response>
/// ```
///
/// # Usage
///
/// After applying the macro to one of these they can be called directly. This will use a default thread pool.
///
/// ```ignore
///     #[derive(Serialize, Deserialize)]
///     pub struct MyRequest;
///     #[derive(Serialize, Deserialize)]
///     pub struct MyResponse;
///     
///     #[worker(TestFutureWorker)]
///     pub async fn future_worker(_req: MyRequest) -> MyResponse {
///         MyResponse
///     }
///
///     // in your component
///     let response = leptos::create_local_resource(|| (), move |_| {
///         future_worker(MyRequest)
///     });
/// ```
///
/// **It is important to use `create_local_resource`** in order to execute the worker locally, as the worker
/// does not exist on the server.
pub use leptos_workers_macro::worker;

pub use plumbing::CreateWorkerError;

#[doc(no_inline)]
pub use flume::Receiver;
#[doc(no_inline)]
pub use flume::Sender;
#[doc(no_inline)]
pub use futures::stream::Stream;

#[doc(hidden)]
pub use futures::future::LocalBoxFuture;
#[doc(hidden)]
pub use futures::stream::LocalBoxStream;
#[doc(hidden)]
pub use futures::stream::StreamExt;
#[doc(hidden)]
pub use wasm_bindgen::prelude::wasm_bindgen;

extern crate alloc;
extern crate core;
