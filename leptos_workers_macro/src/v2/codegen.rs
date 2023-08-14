use crate::v2::lower::Ir;
use proc_macro2::TokenStream;
use quote::quote;

pub type Rust = TokenStream;

pub fn codegen(ir: Ir) -> Rust {
    let Ir {
        worker_struct,
        impl_web_worker,
        impl_web_worker_path,
        wasm_bindgen_func,
        impl_type_worker,
        default_pool_func,
    } = ir;
    quote! {
        #worker_struct
        #impl_web_worker
        #impl_web_worker_path
        #wasm_bindgen_func
        #impl_type_worker
        #default_pool_func
    }
}

#[allow(unused_imports)]
mod tests {
    use crate::v2::codegen;
    use crate::v2::lower::Ir;
    use crate::v2::*;
    use syn::parse_quote;

    #[test]
    fn output_contains_all_parts() {
        let ir = Ir {
            worker_struct: parse_quote!(
                pub struct TestFutureWorker;
            ),
            impl_web_worker: parse_quote!(
                impl ::leptos_workers::workers::WebWorker for TestFutureWorker {
                    type Request = TestRequest;
                    type Response = TestResponse;
                }
            ),
            impl_web_worker_path: parse_quote!(
                impl ::leptos_workers::workers::WebWorkerPath for TestFutureWorker {
                    fn path() -> &'static str {
                        stringify!(TestFutureWorker)
                    }
                }
            ),
            wasm_bindgen_func: parse_quote!(
                #[wasm_bindgen::prelude::wasm_bindgen]
                #[allow(non_snake_case)]
                pub fn WORKERS_FUTURE_TestFutureWorker() -> ::leptos_workers::workers::FutureWorkerFn
                {
                    ::leptos_workers::workers::FutureWorkerFn::new::<TestFutureWorker>()
                }
            ),
            impl_type_worker: parse_quote!(
                impl ::leptos_workers::workers::FutureWorker for TestFutureWorker {
                    fn run(request: Self::Request) -> ::leptos_workers::BoxFuture<'static, Self::Response> {
                        Box::pin(async move {
                            statement1;
                            let statement2 = 0;
                            TestResponse
                        })
                    }
                }
            ),
            default_pool_func: parse_quote!(
                pub async fn future_worker(request: TestRequest) -> TestResponse {
                    thread_local! {
                        static POOL: ::leptos_workers::executors::PoolExecutor<TestFutureWorker> = ::leptos_workers::executors::PoolExecutor::<TestFutureWorker>::new(2).expect("worker creation failed for OptimizeWorkerFuture");
                    }
                    POOL.with(move |pool| pool.run(request))
                        .expect("Worker creation failed for TestFutureWorker")
                        .1
                        .await
                }
            ),
        };
        let rust = codegen::codegen(ir);

        assert!(syn::parse2::<Ir>(rust).is_ok());
    }
}
