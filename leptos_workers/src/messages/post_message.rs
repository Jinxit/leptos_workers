pub trait PostMessage {
    /// Post a message with no transferables.
    fn post_message(&self, to_send: &js_sys::Array);
    /// Post a message with transferables.
    fn post_message_with_transfer(&self, to_send: &js_sys::Array, transferables: &js_sys::Array);
}

impl PostMessage for web_sys::Worker {
    fn post_message(&self, to_send: &js_sys::Array) {
        self.post_message(to_send).expect("post message failed");
    }

    fn post_message_with_transfer(&self, to_send: &js_sys::Array, transferables: &js_sys::Array) {
        self.post_message_with_transfer(to_send, transferables)
            .expect("post message failed");
    }
}

impl PostMessage for web_sys::DedicatedWorkerGlobalScope {
    fn post_message(&self, to_send: &js_sys::Array) {
        self.post_message(to_send).expect("post message failed");
    }

    fn post_message_with_transfer(&self, to_send: &js_sys::Array, transferables: &js_sys::Array) {
        self.post_message_with_transfer(to_send, transferables)
            .expect("post message failed");
    }
}
