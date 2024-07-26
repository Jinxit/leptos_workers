use serde::de::DeserializeOwned;
use serde::Serialize;

#[doc(hidden)]
pub trait WebWorker: WebWorkerPath + Clone + 'static {
    type Request: Clone + Serialize + DeserializeOwned;
    type Response: Clone + Serialize + DeserializeOwned;
}

#[doc(hidden)]
pub trait WebWorkerPath {
    fn path() -> &'static str;
}
