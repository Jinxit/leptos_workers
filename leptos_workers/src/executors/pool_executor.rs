use crate::plumbing::{CreateWorkerError, WorkerHandle};
use crate::workers::CallbackWorker;
use crate::workers::ChannelWorker;
use crate::workers::FutureWorker;
use crate::workers::StreamWorker;
use crate::workers::WebWorker;
use futures::{Stream, StreamExt};
use std::future::Future;
use std::sync::{Arc, Mutex, Weak};
use wasm_bindgen_futures::spawn_local;

/// This executor will run requests on the first available worker in a pool.
///
/// The pool is created with an initial size, but is allowed to expand infinitely if necessary in order to not block.
#[derive(Debug, Clone)]
pub struct PoolExecutor<W: WebWorker> {
    workers: Arc<Mutex<Vec<Arc<Mutex<WorkerPoolState<W>>>>>>,
}

impl<W: WebWorker> PoolExecutor<W> {
    /// # Errors
    /// See [`CreateWorkerError`].
    pub fn new(initial_size: usize) -> Result<Self, CreateWorkerError> {
        Ok(Self {
            workers: Arc::new(Mutex::new(
                (0..initial_size)
                    .map(|id| Ok(Arc::new(Mutex::new(WorkerPoolState::new(id)?))))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
        })
    }

    fn get_or_create_worker(&self) -> Result<Arc<Mutex<WorkerPoolState<W>>>, CreateWorkerError> {
        let worker = self
            .workers
            .lock()
            .unwrap()
            .iter()
            .find(|w| {
                if let Ok(mut worker) = w.try_lock() {
                    if worker.available {
                        worker.available = false;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
            .cloned();
        let worker = worker.map_or_else(
            || {
                let mut workers = self.workers.lock().unwrap();
                let id = workers.len();
                let worker: Arc<Mutex<WorkerPoolState<W>>> =
                    Arc::new(Mutex::new(WorkerPoolState::new(id)?));
                workers.push(worker.clone());
                Ok(worker)
            },
            Ok,
        )?;

        Ok(worker)
    }
}

/// This handle is returned when spawning a worker using a [`PoolExecutor`].
/// It can be used to abort a running worker immediately, leading to creation of
/// a new worker in the pool to replace it.
///
/// **Note**: If this is used in order to abort a worker and then immediately
/// start a new computation, it is better to start the new computation *before*
/// aborting the current one. Otherwise, the new computation will need to wait for
/// the creation of the replaced worker before it can proceed.
#[derive(Clone)]
pub struct AbortHandle<W: WebWorker> {
    worker: Weak<Mutex<WorkerPoolState<W>>>,
    generation: usize,
}

impl<W: WebWorker> AbortHandle<W> {
    /// Aborts an in-progress worker.
    ///
    /// # Panics
    /// This can panic only if there is a logical error in this crate.
    /// The only expected error case is worker creation failure, which is intentionally ignored.
    /// As such, this is not expected to panic for users of the crate.
    pub fn abort(&self) {
        if let Some(ptr) = self.worker.upgrade() {
            let mut worker = ptr.lock().unwrap();
            if !worker.available && worker.generation == self.generation {
                worker.handle.terminate();

                match WorkerPoolState::new(worker.id) {
                    Ok(new_worker) => {
                        *worker = new_worker;
                    }
                    Err(CreateWorkerError::NewWorker(_)) => {
                        // this is most likely because of too many requests
                        // resulting in a cancelled network call, ignore it
                    }
                    Err(e) => {
                        #[allow(clippy::panic)]
                        {
                            panic!("Unexpected error: {e:?}");
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
struct WorkerPoolState<W: WebWorker> {
    handle: WorkerHandle<W>,
    available: bool,
    id: usize,
    generation: usize,
}

impl<W: WebWorker> WorkerPoolState<W> {
    fn new(id: usize) -> Result<Self, CreateWorkerError> {
        Ok(Self {
            handle: WorkerHandle::new()?,
            available: true,
            id,
            generation: 0,
        })
    }
}

impl<W: FutureWorker> PoolExecutor<W> {
    /// Runs a [`FutureWorker`] using this executor.
    ///
    /// # Errors
    /// See [`CreateWorkerError`].
    pub fn run(
        &self,
        request: W::Request,
    ) -> Result<(AbortHandle<W>, impl Future<Output = W::Response>), CreateWorkerError> {
        let worker = self.get_or_create_worker()?;
        let abort_handle = {
            let mut w = worker.lock().unwrap();
            w.generation += 1;
            AbortHandle {
                worker: Arc::downgrade(&worker),
                generation: w.generation,
            }
        };
        let mut handle = worker.lock().unwrap().handle.clone();
        Ok((
            abort_handle,
            release_worker_after_future(worker, async move { handle.run(&request).await }),
        ))
    }
}

impl<W: StreamWorker> PoolExecutor<W> {
    /// Runs a [`StreamWorker`] using this executor.
    ///
    /// # Errors
    /// See [`CreateWorkerError`].
    pub fn stream(
        &self,
        request: &W::Request,
    ) -> Result<(AbortHandle<W>, impl Stream<Item = W::Response>), CreateWorkerError> {
        let worker = self.get_or_create_worker()?;
        let mut w = worker.lock().unwrap();
        w.generation += 1;
        let abort_handle = AbortHandle {
            worker: Arc::downgrade(&worker),
            generation: w.generation,
        };
        Ok((
            abort_handle,
            release_worker_after_stream(worker.clone(), w.handle.stream(request)),
        ))
    }
}

impl<W: CallbackWorker> PoolExecutor<W> {
    /// Runs a [`CallbackWorker`] using this executor.
    ///
    /// # Errors
    /// See [`CreateWorkerError`].
    pub fn stream_callback(
        &self,
        request: W::Request,
        callback: impl Fn(W::Response) + 'static,
    ) -> Result<(AbortHandle<W>, impl Future<Output = ()>), CreateWorkerError> {
        let worker = self.get_or_create_worker()?;
        let abort_handle = {
            let mut w = worker.lock().unwrap();
            w.generation += 1;
            AbortHandle {
                worker: Arc::downgrade(&worker),
                generation: w.generation,
            }
        };
        let mut handle = worker.lock().unwrap().handle.clone();
        Ok((
            abort_handle,
            release_worker_after_future(worker, async move {
                handle.stream_callback(&request, callback).await;
            }),
        ))
    }
}

impl<W: ChannelWorker> PoolExecutor<W> {
    /// Runs a [`ChannelWorker`] using this executor.
    ///
    /// # Errors
    /// See [`CreateWorkerError`].
    pub fn channel(
        &self,
        init: W::Init,
    ) -> Result<
        (
            AbortHandle<W>,
            flume::Sender<W::Request>,
            flume::Receiver<W::Response>,
        ),
        CreateWorkerError,
    > {
        let worker = self.get_or_create_worker()?;
        let (proxy_tx, proxy_rx) = flume::unbounded();
        let (abort_handle, (worker_tx, worker_rx)) = {
            let mut w = worker.lock().unwrap();
            w.generation += 1;
            let abort_handle = AbortHandle {
                worker: Arc::downgrade(&worker),
                generation: w.generation,
            };
            (abort_handle, w.handle.channel(init))
        };
        spawn_local(async move {
            while let Ok(request) = proxy_rx.recv_async().await {
                if worker_tx.send(request).is_err() {
                    break;
                }
            }
            // TODO: is this correct? this and other sending error needs testing
            worker.lock().unwrap().available = true;
        });
        Ok((abort_handle, proxy_tx, worker_rx))
    }
}

fn release_worker_after_future<W: WebWorker, T>(
    worker: Arc<Mutex<WorkerPoolState<W>>>,
    future: impl Future<Output = T>,
) -> impl Future<Output = T> {
    async move {
        let result = future.await;
        worker.lock().unwrap().available = true;
        result
    }
}

fn release_worker_after_stream<W: WebWorker, T>(
    worker: Arc<Mutex<WorkerPoolState<W>>>,
    stream: impl Stream<Item = T>,
) -> impl Stream<Item = T> {
    // this sentinel makes sure we make the worker available only after the stream is done
    let availability_sentinel = Box::pin(futures::stream::unfold(worker, |worker| async move {
        worker.lock().unwrap().available = true;
        None
    }));
    stream.chain(availability_sentinel)
}
