use serde::de::DeserializeOwned;
use serde::Serialize;

pub trait WebWorker: WebWorkerPath + Clone + 'static {
    type Request: Clone + Serialize + DeserializeOwned + Send;
    type Response: Clone + Serialize + DeserializeOwned + Send;
}

pub trait WebWorkerPath {
    fn path() -> &'static str;
}
