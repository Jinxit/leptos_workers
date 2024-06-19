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
/// 1. **Optional**: The name of the worker, typically in PascalCase.
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
/// Define your worker function as such:
///
/// ```
/// # use serde::{Serialize, Deserialize};
/// # #[derive(Clone, Serialize, Deserialize)]
/// # pub struct MyRequest;
/// # #[derive(Clone, Serialize, Deserialize)]
/// # pub struct MyResponse;
/// # use leptos_workers::worker;
/// #[worker(MyCallbackWorker)]
/// /*pub?*/ /*async?*/ fn worker(
///     req: MyRequest,
///     callback: impl Fn(MyResponse)
/// )
/// # { todo!() }
/// ```
///
/// Which will be transformed to:
///
/// ```
/// # pub struct MyRequest;
/// # pub struct MyResponse;
/// /*pub?*/ async fn worker(
///     req: MyRequest,
///     callback: impl Fn(MyResponse) + 'static
/// ) -> Result<(), leptos_workers::CreateWorkerError>
/// # { todo!() }
/// ```
///
/// ## Channel
/// See also: [`ChannelWorker`](crate::workers::ChannelWorker).
///
/// Define your worker function as such:
///
/// ```
/// # use serde::{Serialize, Deserialize};
/// # #[derive(Clone, Serialize, Deserialize)]
/// # pub struct MyRequest;
/// # #[derive(Clone, Serialize, Deserialize)]
/// # pub struct MyResponse;
/// # use leptos_workers::worker;
/// #[worker(MyChannelWorker)]
/// /*pub?*/ /*async?*/ fn worker(
///     rx: leptos_workers::Receiver<MyRequest>,
///     tx: leptos_workers::Sender<MyResponse>
/// )
/// # { todo!() }
/// ```
///
/// Which will be transformed to:
///
/// ```
/// # pub struct MyRequest;
/// # pub struct MyResponse;
/// /*pub?*/ fn worker() -> Result<
///     (
///         leptos_workers::Sender<MyRequest>,
///         leptos_workers::Receiver<MyResponse>,
///     ),
///     leptos_workers::CreateWorkerError,
/// >
/// # { todo!() }
/// ```
///
/// ## Future
/// See also: [`FutureWorker`](crate::workers::FutureWorker).
///
/// Define your worker function as such:
///
/// ```
/// # use serde::{Serialize, Deserialize};
/// # #[derive(Clone, Serialize, Deserialize)]
/// # pub struct MyRequest;
/// # #[derive(Clone, Serialize, Deserialize)]
/// # pub struct MyResponse;
/// # use leptos_workers::worker;
/// #[worker(MyFutureWorker)]
/// /*pub?*/ /*async?*/ fn worker(
///     req: MyRequest
/// ) -> MyResponse
/// # { todo!() }
/// ```
///
/// Which will be transformed to:
///
/// ```
/// # pub struct MyRequest;
/// # pub struct MyResponse;
/// /*pub?*/ async fn worker(
///     req: MyRequest,
/// ) -> Result<MyResponse, leptos_workers::CreateWorkerError>
/// # { todo!() }
/// ```
///
/// ## Stream
/// See also: [`StreamWorker`](crate::workers::StreamWorker).
///
/// Define your worker function as such:
///
/// ```
/// # use serde::{Serialize, Deserialize};
/// # #[derive(Clone, Serialize, Deserialize)]
/// # pub struct MyRequest;
/// # #[derive(Clone, Serialize, Deserialize)]
/// # pub struct MyResponse;
/// # use leptos_workers::worker;
/// #[worker(MyStreamWorker)]
/// /*pub?*/ /*async?*/ fn worker(
///     req: MyRequest
/// ) -> impl leptos_workers::Stream<Item = MyResponse>
/// # { futures::stream::once(async { MyResponse }) }
/// ```
///
/// Which will be transformed to:
///
/// ```
/// # pub struct MyRequest;
/// # pub struct MyResponse;
/// /*pub?*/ fn worker(
///     req: &MyRequest,
/// ) -> Result<
///     impl leptos_workers::Stream<Item = MyResponse>,
///     leptos_workers::CreateWorkerError
/// >
/// # { Ok(futures::stream::once(async { MyResponse })) }
/// ```
///
/// # Usage
///
/// After applying the macro to one of the above examples they can be called directly. This will use a default thread pool.
/// Don't forget to handle the error case if you need compatibility with older browsers.
///
/// ```
/// # use serde::{Serialize, Deserialize};
/// # use leptos_workers::worker;
/// # fn create_local_resource<T>(f: fn(), g: fn(Option<T>) -> T) {}
/// #[derive(Clone, Serialize, Deserialize)]
/// pub struct MyRequest;
/// #[derive(Clone, Serialize, Deserialize)]
/// pub struct MyResponse;
///
/// #[worker(MyFutureWorker)]
/// pub async fn future_worker(_req: MyRequest) -> MyResponse {
///     MyResponse
/// }
/// # fn main() {
/// // in your component
/// let response = create_local_resource(|| (), move |_| {
///     future_worker(MyRequest)
/// });
/// # }
/// ```
///
/// **If using SSR it is important to ensure the worker only executes on the client**,
/// as the worker does not exist on the server. A few examples of wrapping functions include:
/// - `create_local_resource`
/// - `create_effect`
/// - `create_action`
/// - `spawn_local`
///
/// If using pure CSR, this is not a problem.
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
