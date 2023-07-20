use crate::plumbing::{CreateWorkerError, WorkerHandle};
use crate::workers::callback_worker::CallbackWorker;
use crate::workers::channel_worker::ChannelWorker;
use crate::workers::future_worker::FutureWorker;
use crate::workers::stream_worker::StreamWorker;
use crate::workers::web_worker::WebWorker;
use alloc::rc::{Rc, Weak};
use futures::{Stream, StreamExt};
use std::cell::RefCell;
use std::future::Future;
use wasm_bindgen_futures::spawn_local;

#[derive(Debug, Clone)]
pub struct PoolExecutor<W: WebWorker> {
    workers: RefCell<Vec<Rc<RefCell<WorkerPoolState<W>>>>>,
}

impl<W: WebWorker> PoolExecutor<W> {
    pub fn new(initial_size: usize) -> Result<Self, CreateWorkerError> {
        Ok(Self {
            workers: RefCell::new(
                (0..initial_size)
                    .map(|id| Ok(Rc::new(RefCell::new(WorkerPoolState::new(id)?))))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        })
    }

    fn get_or_create_worker(&self) -> Result<Rc<RefCell<WorkerPoolState<W>>>, CreateWorkerError> {
        let worker = self
            .workers
            .borrow()
            .iter()
            .find(|w| {
                if let Ok(mut worker) = w.try_borrow_mut() {
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
                let id = self.workers.borrow().len();
                let worker: Rc<RefCell<WorkerPoolState<W>>> =
                    Rc::new(RefCell::new(WorkerPoolState::new(id)?));
                self.workers.borrow_mut().push(worker.clone());
                Ok(worker)
            },
            Ok,
        )?;

        Ok(worker)
    }
}

#[derive(Clone)]
pub struct AbortHandle<W: WebWorker> {
    worker: Weak<RefCell<WorkerPoolState<W>>>,
    generation: usize,
}

impl<W: WebWorker> AbortHandle<W> {
    pub fn abort(&self) {
        if let Some(ptr) = self.worker.upgrade() {
            let mut worker = ptr.borrow_mut();
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
    pub fn run(
        &self,
        request: W::Request,
    ) -> Result<(AbortHandle<W>, impl Future<Output = W::Response>), CreateWorkerError> {
        let worker = self.get_or_create_worker()?;
        let abort_handle = {
            let mut w = worker.borrow_mut();
            w.generation += 1;
            AbortHandle {
                worker: Rc::downgrade(&worker),
                generation: w.generation,
            }
        };
        let mut handle = worker.borrow_mut().handle.clone();
        Ok((
            abort_handle,
            release_worker_after_future(worker, async move { handle.run(&request).await }),
        ))
    }
}

impl<W: StreamWorker> PoolExecutor<W> {
    pub fn stream(
        &self,
        request: &W::Request,
    ) -> Result<(AbortHandle<W>, impl Stream<Item = W::Response>), CreateWorkerError> {
        let worker = self.get_or_create_worker()?;
        let mut w = worker.borrow_mut();
        w.generation += 1;
        let abort_handle = AbortHandle {
            worker: Rc::downgrade(&worker),
            generation: w.generation,
        };
        Ok((
            abort_handle,
            release_worker_after_stream(worker.clone(), w.handle.stream(request)),
        ))
    }
}

impl<W: CallbackWorker> PoolExecutor<W> {
    pub fn stream_callback(
        &self,
        request: W::Request,
        callback: impl Fn(W::Response) + 'static,
    ) -> Result<(AbortHandle<W>, impl Future<Output = ()>), CreateWorkerError> {
        let worker = self.get_or_create_worker()?;
        let abort_handle = {
            let mut w = worker.borrow_mut();
            w.generation += 1;
            AbortHandle {
                worker: Rc::downgrade(&worker),
                generation: w.generation,
            }
        };
        let mut handle = worker.borrow_mut().handle.clone();
        Ok((
            abort_handle,
            release_worker_after_future(worker, async move {
                handle.stream_callback(&request, callback).await;
            }),
        ))
    }
}

impl<W: ChannelWorker> PoolExecutor<W> {
    pub fn channel(
        &self,
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
            let mut w = worker.borrow_mut();
            w.generation += 1;
            let abort_handle = AbortHandle {
                worker: Rc::downgrade(&worker),
                generation: w.generation,
            };
            (abort_handle, w.handle.channel())
        };
        spawn_local(async move {
            while let Ok(request) = proxy_rx.recv_async().await {
                if worker_tx.send(request).is_err() {
                    break;
                }
            }
            // TODO: is this correct? this and other sending error needs testing
            worker.borrow_mut().available = true;
        });
        Ok((abort_handle, proxy_tx, worker_rx))
    }
}

fn release_worker_after_future<W: WebWorker, T>(
    worker: Rc<RefCell<WorkerPoolState<W>>>,
    future: impl Future<Output = T>,
) -> impl Future<Output = T> {
    async move {
        let result = future.await;
        worker.borrow_mut().available = true;
        result
    }
}

fn release_worker_after_stream<W: WebWorker, T>(
    worker: Rc<RefCell<WorkerPoolState<W>>>,
    stream: impl Stream<Item = T>,
) -> impl Stream<Item = T> {
    // this sentinel makes sure we make the worker available only after the stream is done
    let availability_sentinel = Box::pin(futures::stream::unfold(worker, |worker| async move {
        worker.borrow_mut().available = true;
        None
    }));
    stream.chain(availability_sentinel)
}
