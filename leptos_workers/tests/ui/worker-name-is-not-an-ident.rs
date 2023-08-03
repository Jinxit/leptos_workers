use leptos_workers_macro::worker;

#[worker(struct)]
async fn future_worker(req: TestRequest) -> TestResponse {}

fn main() {}
