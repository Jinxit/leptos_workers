use crate::plumbing::{CreateWorkerError, WorkerHandle};
use crate::workers::callback_worker::CallbackWorker;
use crate::workers::channel_worker::ChannelWorker;
use crate::workers::future_worker::FutureWorker;
use crate::workers::stream_worker::StreamWorker;
use crate::workers::web_worker::WebWorker;
use futures::Stream;

// TODO: this used to have more of a reason to exist - delete?

#[derive(Debug, Clone)]
pub struct SingleExecutor<W: WebWorker> {
    handle: WorkerHandle<W>,
}

impl<W: WebWorker> SingleExecutor<W> {
    pub fn new() -> Result<Self, CreateWorkerError> {
        Ok(Self {
            handle: WorkerHandle::new()?,
        })
    }
}

impl<W: FutureWorker> SingleExecutor<W> {
    pub async fn run(&mut self, request: &W::Request) -> W::Response {
        self.handle.run(request).await
    }
}

impl<W: StreamWorker> SingleExecutor<W> {
    pub fn stream(&mut self, request: &W::Request) -> impl Stream<Item = W::Response> {
        self.handle.stream(request)
    }
}

impl<W: CallbackWorker> SingleExecutor<W> {
    pub async fn stream_callback(
        &mut self,
        request: &W::Request,
        callback: impl Fn(W::Response) + 'static,
    ) {
        self.handle.stream_callback(request, callback).await;
    }
}

impl<W: ChannelWorker> SingleExecutor<W> {
    #[must_use]
    pub fn channel(&mut self) -> (flume::Sender<W::Request>, flume::Receiver<W::Response>) {
        self.handle.channel()
    }
}
