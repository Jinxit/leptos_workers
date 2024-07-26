use crate::plumbing::{CreateWorkerError, WorkerHandle};
use crate::workers::CallbackWorker;
use crate::workers::ChannelWorker;
use crate::workers::FutureWorker;
use crate::workers::StreamWorker;
use crate::workers::WebWorker;
use futures::Stream;

// TODO: this used to have more of a reason to exist - delete?

/// This simple executor will run requests on a single worker.
/// Sending multiple requests to the same worker may or may not work, and is not recommended.
///
/// Honestly, using this executor is not recommended, just use a [`PoolExecutor`](crate::executors::PoolExecutor) instead.
#[derive(Debug, Clone)]
pub struct SingleExecutor<W: WebWorker> {
    handle: WorkerHandle<W>,
}

impl<W: WebWorker> SingleExecutor<W> {
    /// # Errors
    /// See [`CreateWorkerError`].
    pub fn new() -> Result<Self, CreateWorkerError> {
        Ok(Self {
            handle: WorkerHandle::new()?,
        })
    }
}

impl<W: FutureWorker> SingleExecutor<W> {
    /// Runs a [`FutureWorker`] using this executor.
    ///
    /// # Errors
    /// See [`CreateWorkerError`].
    pub async fn run(&mut self, request: &W::Request) -> W::Response {
        self.handle.run(request).await
    }
}

impl<W: StreamWorker> SingleExecutor<W> {
    /// Runs a [`StreamWorker`] using this executor.
    ///
    /// # Errors
    /// See [`CreateWorkerError`].
    pub fn stream(&mut self, request: &W::Request) -> impl Stream<Item = W::Response> {
        self.handle.stream(request)
    }
}

impl<W: CallbackWorker> SingleExecutor<W> {
    /// Runs a [`CallbackWorker`] using this executor.
    ///
    /// # Errors
    /// See [`CreateWorkerError`].
    pub async fn stream_callback(
        &mut self,
        request: &W::Request,
        callback: impl Fn(W::Response) + 'static,
    ) {
        self.handle.stream_callback(request, callback).await;
    }
}

impl<W: ChannelWorker> SingleExecutor<W> {
    /// Runs a [`ChannelWorker`] using this executor.
    ///
    /// # Errors
    /// See [`CreateWorkerError`].
    #[must_use]
    pub fn channel(
        &mut self,
        init: W::Init,
    ) -> (flume::Sender<W::Request>, flume::Receiver<W::Response>) {
        self.handle.channel(init)
    }
}
