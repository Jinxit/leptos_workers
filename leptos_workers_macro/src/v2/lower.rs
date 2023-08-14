use crate::v2::analyze::{
    Model, WorkerType, WorkerTypeCallback, WorkerTypeChannel, WorkerTypeFuture, WorkerTypeStream,
};
use proc_macro2::Ident;
use proc_macro2::TokenStream;
use quote::{format_ident, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{parse_quote, parse_quote_spanned, ItemFn, ItemImpl, ItemStruct, Stmt};

pub struct Ir {
    pub worker_struct: ItemStruct,
    pub impl_web_worker: ItemImpl,
    pub impl_web_worker_path: ItemImpl,
    pub wasm_bindgen_func: ItemFn,
    pub impl_type_worker: ItemImpl,
    pub default_pool_func: ItemFn,
}

impl Parse for Ir {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Ir {
            worker_struct: input.parse()?,
            impl_web_worker: input.parse()?,
            impl_web_worker_path: input.parse()?,
            wasm_bindgen_func: input.parse()?,
            impl_type_worker: input.parse()?,
            default_pool_func: input.parse()?,
        })
    }
}

pub fn lower(model: &Model) -> Ir {
    let worker_struct = lower_worker_struct(model);
    let impl_web_worker = lower_impl_web_worker(model);
    let impl_web_worker_path = lower_impl_web_worker_path(model);
    let wasm_bindgen_func = lower_wasm_bindgen_func(model);
    let (default_pool_func, impl_type_worker) = lower_impl_type_worker(model);

    Ir {
        worker_struct,
        impl_web_worker,
        impl_web_worker_path,
        wasm_bindgen_func,
        impl_type_worker,
        default_pool_func,
    }
}

fn lower_worker_struct(model: &Model) -> ItemStruct {
    let Model {
        worker_name,
        visibility,
        ..
    } = model;
    parse_quote_spanned!(worker_name.span()=>
        #[derive(Debug, Clone)]
        #visibility struct #worker_name;
    )
}

fn lower_impl_web_worker(model: &Model) -> ItemImpl {
    let Model {
        worker_name,
        worker_type,
        ..
    } = model;
    let request_type = worker_type.request_type();
    let response_type = worker_type.response_type();
    parse_quote!(
        impl ::leptos_workers::workers::WebWorker for #worker_name {
            type Request = #request_type;
            type Response = #response_type;
        }
    )
}

fn lower_impl_web_worker_path(model: &Model) -> ItemImpl {
    let Model { worker_name, .. } = model;
    let worker_name_str = worker_name.to_string();
    parse_quote!(
        impl ::leptos_workers::workers::WebWorkerPath for #worker_name {
            fn path() -> &'static str {
                #worker_name_str
            }
        }
    )
}

fn lower_wasm_bindgen_func(model: &Model) -> ItemFn {
    let Model {
        worker_name,
        worker_type,
        ..
    } = model;
    let prefix = worker_type.wasm_function_prefix();
    let func_name = format_ident!("{prefix}_{worker_name}");
    let worker_fn_type = worker_type.worker_fn_type();
    parse_quote!(
        #[::leptos_workers::wasm_bindgen]
        #[allow(non_snake_case)]
        pub fn #func_name() -> ::leptos_workers::workers::#worker_fn_type {
            ::leptos_workers::workers::#worker_fn_type::new::<#worker_name>()
        }
    )
}

fn lower_impl_type_worker(model: &Model) -> (ItemFn, ItemImpl) {
    match &model.worker_type {
        WorkerType::Callback(callback) => lower_impl_type_worker_callback(model, callback),
        WorkerType::Channel(channel) => lower_impl_type_worker_channel(model, channel),
        WorkerType::Future(future) => lower_impl_type_worker_future(model, future),
        WorkerType::Stream(stream) => lower_impl_type_worker_stream(model, stream),
    }
}

fn lower_impl_type_worker_callback(
    model: &Model,
    callback: &WorkerTypeCallback,
) -> (ItemFn, ItemImpl) {
    let Model {
        worker_name,
        visibility,
        function_name,
        function_body,
        ..
    } = model;

    let WorkerTypeCallback {
        request_pat,
        request_type,
        response_type,
        callback_pat,
        ..
    } = callback;

    let create_error_msg = create_error_msg(worker_name);
    let thread_pool = thread_pool(model, &create_error_msg);

    let func = parse_quote!(
        #visibility async fn #function_name(
            #request_pat: #request_type,
            #callback_pat: impl Fn(#response_type) + 'static,
        ) {
            #thread_pool
            POOL.with(move |pool| pool.stream_callback(#request_pat, Box::new(#callback_pat))).expect(#create_error_msg).1.await
        }
    );

    let imp = parse_quote!(
        impl ::leptos_workers::workers::CallbackWorker for #worker_name {
            fn stream_callback(#request_pat: Self::Request, #callback_pat: Box<dyn Fn(Self::Response)>) -> ::leptos_workers::LocalBoxFuture<'static, ()> {
                Box::pin(async move {
                    #(#function_body)*
                })
            }
        }
    );

    (func, imp)
}

fn lower_impl_type_worker_channel(
    model: &Model,
    channel: &WorkerTypeChannel,
) -> (ItemFn, ItemImpl) {
    let Model {
        worker_name,
        visibility,
        function_name,
        function_body,
        ..
    } = model;

    let WorkerTypeChannel {
        receiver_pat,
        request_type,
        sender_pat,
        response_type,
        ..
    } = channel;

    let create_error_msg = create_error_msg(worker_name);
    let thread_pool = thread_pool(model, &create_error_msg);

    let func = parse_quote!(
        #visibility async fn #function_name() -> (::leptos_workers::Sender<#request_type>, ::leptos_workers::Receiver<#response_type>) {
            #thread_pool
            let (_, tx, rx) = POOL.with(move |pool| pool.channel()).expect(#create_error_msg);
            (tx, rx)
        }
    );

    let imp = parse_quote!(
        impl ::leptos_workers::workers::ChannelWorker for #worker_name {
            fn channel(#receiver_pat: ::leptos_workers::Receiver<Self::Request>, #sender_pat: ::leptos_workers::Sender<Self::Response>) -> ::leptos_workers::LocalBoxFuture<'static, ()>  {
                Box::pin(async move {
                    #(#function_body)*
                })
            }
        }
    );

    (func, imp)
}

fn lower_impl_type_worker_future(model: &Model, future: &WorkerTypeFuture) -> (ItemFn, ItemImpl) {
    let Model {
        worker_name,
        visibility,
        function_name,
        function_body,
        ..
    } = model;

    let WorkerTypeFuture {
        request_pat,
        request_type,
        response_type,
        ..
    } = future;

    let create_error_msg = create_error_msg(worker_name);
    let thread_pool = thread_pool(model, &create_error_msg);

    let func = parse_quote!(
        #visibility async fn #function_name(
            #request_pat: #request_type,
        ) -> #response_type {
            #thread_pool
            POOL.with(move |pool| pool.run(#request_pat)).expect(#create_error_msg).1.await
        }
    );

    let imp = parse_quote!(
        impl ::leptos_workers::workers::FutureWorker for #worker_name {
            fn run(#request_pat: Self::Request) -> ::leptos_workers::LocalBoxFuture<'static, Self::Response> {
                async fn inner(#request_pat: #request_type) -> #response_type {
                    #(#function_body)*
                }

                Box::pin(async move {
                    inner(#request_pat).await
                })
            }
        }
    );

    (func, imp)
}

fn lower_impl_type_worker_stream(model: &Model, stream: &WorkerTypeStream) -> (ItemFn, ItemImpl) {
    let Model {
        worker_name,
        visibility,
        function_name,
        function_body,
        ..
    } = model;

    let WorkerTypeStream {
        request_pat,
        request_type,
        response_type,
        return_type,
        ..
    } = stream;

    let create_error_msg = create_error_msg(worker_name);
    let thread_pool = thread_pool(model, &create_error_msg);

    let response_type_spanned: TokenStream =
        quote_spanned!(return_type.span()=> impl ::leptos_workers::Stream<Item = #response_type>);
    let func = parse_quote!(
        #visibility fn #function_name(
            #request_pat: &#request_type,
        ) -> #response_type_spanned {
            #thread_pool
            POOL.with(move |pool| pool.stream(#request_pat)).expect(#create_error_msg).1
        }
    );

    let return_type_spanned: TokenStream = quote_spanned!(return_type.span()=> ::leptos_workers::LocalBoxStream<'static, Self::Response>);
    let imp = parse_quote!(
        impl ::leptos_workers::workers::StreamWorker for #worker_name {
            fn stream(#request_pat: Self::Request) -> #return_type_spanned {
                fn inner(#request_pat: #request_type) -> #return_type {
                    #(#function_body)*
                }

                use ::leptos_workers::StreamExt;
                inner(#request_pat).boxed_local()
            }
        }
    );

    (func, imp)
}

fn thread_pool(model: &Model, creation_error_msg: &str) -> Stmt {
    let Model {
        worker_name,
        worker_type,
        ..
    } = model;
    let pool_size = worker_type.default_pool_size();
    parse_quote! {
        thread_local! {
            static POOL: ::leptos_workers::executors::PoolExecutor<#worker_name> =
                ::leptos_workers::executors::PoolExecutor::<#worker_name>::new(#pool_size).expect(#creation_error_msg);
        }
    }
}

fn create_error_msg(worker_name: &Ident) -> String {
    format!("worker creation failed for {worker_name}")
}

#[allow(unused_imports)]
mod tests {
    use super::*;
    use crate::v2::analyze::WorkerType;
    use crate::v2::lower::lower;
    use pretty_assertions::assert_eq;
    use syn::parse_quote;

    /* lower */

    #[test]
    fn produces_future_worker_ir() {
        let model = Model {
            worker_name: parse_quote!(TestFutureWorker),
            worker_type: WorkerType::Future(WorkerTypeFuture {
                request_pat: parse_quote!(request),
                request_type: parse_quote!(TestRequest),
                response_type: parse_quote!(TestResponse),
            }),
            visibility: parse_quote!(pub),
            function_name: parse_quote!(future_worker),
            function_body: parse_quote!(
                statement1;
                let statement2 = 0;
                TestResponse
            ),
        };
        let ir = lower(&model);

        let expected: ItemStruct = parse_quote!(
            #[derive(Debug, Clone)]
            pub struct TestFutureWorker;
        );
        assert_eq!(expected, ir.worker_struct);

        let expected: ItemImpl = parse_quote!(
            impl ::leptos_workers::workers::WebWorker for TestFutureWorker {
                type Request = TestRequest;
                type Response = TestResponse;
            }
        );
        assert_eq!(expected, ir.impl_web_worker);

        let expected: ItemImpl = parse_quote!(
            impl ::leptos_workers::workers::WebWorkerPath for TestFutureWorker {
                fn path() -> &'static str {
                    "TestFutureWorker"
                }
            }
        );
        assert_eq!(expected, ir.impl_web_worker_path);

        let expected: ItemFn = parse_quote!(
            #[::leptos_workers::wasm_bindgen]
            #[allow(non_snake_case)]
            pub fn WORKERS_FUTURE_TestFutureWorker() -> ::leptos_workers::workers::FutureWorkerFn {
                ::leptos_workers::workers::FutureWorkerFn::new::<TestFutureWorker>()
            }
        );
        assert_eq!(expected, ir.wasm_bindgen_func);

        let expected: ItemImpl = parse_quote!(
            impl ::leptos_workers::workers::FutureWorker for TestFutureWorker {
                fn run(request: Self::Request) -> ::leptos_workers::LocalBoxFuture<'static, Self::Response> {
                    async fn inner(request: TestRequest) -> TestResponse {
                        statement1;
                        let statement2 = 0;
                        TestResponse
                    }
                    Box::pin(async move {
                        inner(request).await
                    })
                }
            }
        );
        assert_eq!(expected, ir.impl_type_worker);

        let creation_error_msg = "worker creation failed for TestFutureWorker";
        let expected: ItemFn = parse_quote!(
            pub async fn future_worker(
                request: TestRequest,
            ) -> TestResponse {
                thread_local! {
                    static POOL: ::leptos_workers::executors::PoolExecutor<TestFutureWorker> =
                        ::leptos_workers::executors::PoolExecutor::<TestFutureWorker>::new(2usize).expect(#creation_error_msg);
                }
                POOL.with(move |pool| pool.run(request)).expect(#creation_error_msg).1.await
            }
        );
        assert_eq!(expected, ir.default_pool_func);
    }
}
