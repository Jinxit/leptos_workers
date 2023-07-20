use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum WorkerRequestType {
    Future,
    Stream,
    Callback,
    Channel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerRequest {
    pub request_type: WorkerRequestType,
    pub request_data: Vec<u8>,
}
