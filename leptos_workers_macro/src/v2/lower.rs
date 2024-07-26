use crate::v2::analyze::{
    Model, WorkerType, WorkerTypeCallback, WorkerTypeChannel, WorkerTypeFuture, WorkerTypeStream,
};
use proc_macro2::Ident;
use proc_macro2::TokenStream;
use proc_macro_error::abort;
use quote::{format_ident, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::parse_quote;
use syn::spanned::Spanned;
use syn::Pat;
use syn::{parse_quote_spanned, ItemFn, ItemImpl, ItemStruct, Stmt};

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
    parse_quote_spanned!(worker_name.span()=>
        impl ::leptos_workers::workers::WebWorker for #worker_name {
            type Request = #request_type;
            type Response = #response_type;
        }
    )
}

fn lower_impl_web_worker_path(model: &Model) -> ItemImpl {
    let Model { worker_name, .. } = model;
    let worker_name_str = worker_name.to_string();
    parse_quote_spanned!(worker_name.span()=>
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
    parse_quote_spanned!(worker_name.span()=>
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
        function_attrs,
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

    let request_pat_name = get_ident_from_pat(request_pat);
    let callback_pat_name = get_ident_from_pat(callback_pat);

    let thread_pool = thread_pool(model, &[request_pat_name.clone(), callback_pat_name]);

    let func = parse_quote_spanned!(worker_name.span()=>
        #(#function_attrs)*
        #visibility async fn #function_name(
            #request_pat_name: #request_type,
            #callback_pat: impl Fn(#response_type) + 'static,
        ) -> Result<(), ::leptos_workers::CreateWorkerError> {
            #(#thread_pool)*
            Ok(POOL
                .with(move |pool| {
                    pool.as_ref()
                        .map_err(Clone::clone)
                        .and_then(|pool| pool.stream_callback(#request_pat_name, Box::new(#callback_pat)))
                })?
                .1
                .await)
        }
    );

    let imp = parse_quote_spanned!(worker_name.span()=>
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
        function_attrs,
        function_name,
        function_body,
        ..
    } = model;

    let WorkerTypeChannel {
        init_pat,
        init_type,
        receiver_pat,
        request_type,
        sender_pat,
        response_type,
        ..
    } = channel;

    let thread_pool = thread_pool(model, &[]);

    let func = parse_quote_spanned!(worker_name.span()=>
        #(#function_attrs)*
        #visibility fn #function_name(#init_pat: #init_type) -> Result<(::leptos_workers::Sender<#request_type>, ::leptos_workers::Receiver<#response_type>), ::leptos_workers::CreateWorkerError> {
            #(#thread_pool)*
            let (_, tx, rx) = POOL
                .with(move |pool| {
                    pool.as_ref()
                        .map_err(Clone::clone)
                        .and_then(|pool| pool.channel(#init_pat))
                })?;
            Ok((tx, rx))
        }
    );

    let imp = parse_quote_spanned!(worker_name.span()=>
        impl ::leptos_workers::workers::ChannelWorker for #worker_name {
            type Init = #init_type;

            fn channel(#init_pat: #init_type, #receiver_pat: ::leptos_workers::Receiver<Self::Request>, #sender_pat: ::leptos_workers::Sender<Self::Response>) -> ::leptos_workers::LocalBoxFuture<'static, ()>  {
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
        function_attrs,
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

    let request_pat_name = get_ident_from_pat(request_pat);

    let thread_pool = thread_pool(model, &[request_pat_name.clone()]);

    let func = parse_quote_spanned!(worker_name.span()=>
        #(#function_attrs)*
        #visibility async fn #function_name(
            #request_pat_name: #request_type,
        ) -> Result<#response_type, ::leptos_workers::CreateWorkerError> {
            #(#thread_pool)*
            Ok(POOL
                .with(move |pool| {
                    pool.as_ref()
                        .map_err(Clone::clone)
                        .and_then(|pool| pool.run(#request_pat_name))
                })?
                .1
                .await)
        }
    );

    let imp = parse_quote_spanned!(worker_name.span()=>
        impl ::leptos_workers::workers::FutureWorker for #worker_name {
            fn run(#request_pat_name: Self::Request) -> ::leptos_workers::LocalBoxFuture<'static, Self::Response> {
                async fn inner(#request_pat: #request_type) -> #response_type {
                    #(#function_body)*
                }

                Box::pin(async move {
                    inner(#request_pat_name).await
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
        function_attrs,
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

    let request_pat_name = get_ident_from_pat(request_pat);

    let thread_pool = thread_pool(model, &[request_pat_name.clone()]);

    let response_type_spanned: TokenStream =
        quote_spanned!(return_type.span()=> impl ::leptos_workers::Stream<Item = #response_type>);
    let func = parse_quote_spanned!(worker_name.span()=>
        #(#function_attrs)*
        #visibility fn #function_name(
            #request_pat_name: &#request_type,
        ) -> Result<#response_type_spanned, ::leptos_workers::CreateWorkerError> {
            #(#thread_pool)*
            Ok(POOL
                .with(move |pool| {
                    pool.as_ref()
                        .map_err(Clone::clone)
                        .and_then(|pool| pool.stream(#request_pat_name))
                })?
                .1)
        }
    );

    let return_type_spanned: TokenStream = quote_spanned!(return_type.span()=> ::leptos_workers::LocalBoxStream<'static, Self::Response>);
    let imp = parse_quote_spanned!(worker_name.span()=>
        impl ::leptos_workers::workers::StreamWorker for #worker_name {
            fn stream(#request_pat_name: Self::Request) -> #return_type_spanned {
                fn inner(#request_pat: #request_type) -> #return_type {
                    #(#function_body)*
                }

                use ::leptos_workers::StreamExt;
                inner(#request_pat_name).boxed_local()
            }
        }
    );

    (func, imp)
}

fn thread_pool(model: &Model, inputs: &[Ident]) -> Vec<Stmt> {
    let Model {
        worker_name,
        worker_type,
        ..
    } = model;
    let pool_size = worker_type.default_pool_size();

    let ssr_check = ssr_check(inputs);

    parse_quote_spanned!(worker_name.span()=>
        #(#ssr_check)*
        thread_local! {
            static POOL: Result<::leptos_workers::executors::PoolExecutor<#worker_name>, ::leptos_workers::CreateWorkerError> =
                ::leptos_workers::executors::PoolExecutor::<#worker_name>::new(#pool_size);
        }
    )
}

fn ssr_check(inputs: &[Ident]) -> Vec<Stmt> {
    if cfg!(feature = "ssr") {
        parse_quote!(
            #(let _ = #inputs;)*
            if true {
                panic!("Workers can't be constructed on the server. Try using `create_effect`, `create_local_resource`, `create_action` or `spawn_local`.")
            }
        )
    } else {
        parse_quote!()
    }
}

fn get_ident_from_pat(request_pat: &Pat) -> Ident {
    if let Pat::Ident(ident) = request_pat {
        ident.ident.clone()
    } else {
        abort!(request_pat, "unexpected pattern type")
    }
}

#[allow(unused_imports)]
mod tests {
    use super::*;
    use crate::v2::analyze::WorkerType;
    use crate::v2::lower::lower;
    use pretty_assertions::assert_eq;
    use quote::ToTokens;
    use syn::parse_quote;

    #[allow(dead_code)]
    fn assert_eq_quoted<L: ToTokens + PartialEq<R>, R: ToTokens>(lhs: &L, rhs: &R) {
        if lhs != rhs {
            assert_eq!(
                quote::quote!(#lhs).to_string(),
                quote::quote!(#rhs).to_string()
            );
        }
    }

    #[test]
    #[cfg(not(feature = "ssr"))]
    fn produces_future_worker_ir() {
        let model = Model {
            worker_name: parse_quote!(TestFutureWorker),
            worker_type: WorkerType::Future(WorkerTypeFuture {
                request_pat: parse_quote!(request),
                request_type: parse_quote!(TestRequest),
                response_type: parse_quote!(TestResponse),
            }),
            visibility: parse_quote!(pub),
            function_attrs: vec![parse_quote!(
                /// function-level doc
            )],
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
        assert_eq_quoted(&expected, &ir.worker_struct);

        let expected: ItemImpl = parse_quote!(
            impl ::leptos_workers::workers::WebWorker for TestFutureWorker {
                type Request = TestRequest;
                type Response = TestResponse;
            }
        );
        assert_eq_quoted(&expected, &ir.impl_web_worker);

        let expected: ItemImpl = parse_quote!(
            impl ::leptos_workers::workers::WebWorkerPath for TestFutureWorker {
                fn path() -> &'static str {
                    "TestFutureWorker"
                }
            }
        );
        assert_eq_quoted(&expected, &ir.impl_web_worker_path);

        let expected: ItemFn = parse_quote!(
            #[::leptos_workers::wasm_bindgen]
            #[allow(non_snake_case)]
            pub fn WORKERS_FUTURE_TestFutureWorker() -> ::leptos_workers::workers::FutureWorkerFn {
                ::leptos_workers::workers::FutureWorkerFn::new::<TestFutureWorker>()
            }
        );
        assert_eq_quoted(&expected, &ir.wasm_bindgen_func);

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
        assert_eq_quoted(&expected, &ir.impl_type_worker);

        let expected: ItemFn = parse_quote!(
            #[doc = r" function-level doc"]
            pub async fn future_worker(
                request: TestRequest,
            ) -> Result<TestResponse, ::leptos_workers::CreateWorkerError> {
                thread_local! {
                    static POOL: Result<::leptos_workers::executors::PoolExecutor<TestFutureWorker>, ::leptos_workers::CreateWorkerError> =
                        ::leptos_workers::executors::PoolExecutor::<TestFutureWorker>::new(2usize);
                }
                Ok(POOL
                    .with(move |pool| {
                        pool.as_ref()
                            .map_err(Clone::clone)
                            .and_then(|pool| pool.run(request))
                    })?
                    .1
                    .await)
            }
        );
        assert_eq_quoted(&expected, &ir.default_pool_func);
    }

    #[test]
    #[cfg(not(feature = "ssr"))]
    fn produces_future_worker_ir_with_mut_arg() {
        let model = Model {
            worker_name: parse_quote!(TestFutureWorker),
            worker_type: WorkerType::Future(WorkerTypeFuture {
                request_pat: parse_quote!(mut request),
                request_type: parse_quote!(TestRequest),
                response_type: parse_quote!(TestResponse),
            }),
            visibility: parse_quote!(pub),
            function_attrs: vec![parse_quote!(
                /// function-level doc
            )],
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
        assert_eq_quoted(&expected, &ir.worker_struct);

        let expected: ItemImpl = parse_quote!(
            impl ::leptos_workers::workers::WebWorker for TestFutureWorker {
                type Request = TestRequest;
                type Response = TestResponse;
            }
        );
        assert_eq_quoted(&expected, &ir.impl_web_worker);

        let expected: ItemImpl = parse_quote!(
            impl ::leptos_workers::workers::WebWorkerPath for TestFutureWorker {
                fn path() -> &'static str {
                    "TestFutureWorker"
                }
            }
        );
        assert_eq_quoted(&expected, &ir.impl_web_worker_path);

        let expected: ItemFn = parse_quote!(
            #[::leptos_workers::wasm_bindgen]
            #[allow(non_snake_case)]
            pub fn WORKERS_FUTURE_TestFutureWorker() -> ::leptos_workers::workers::FutureWorkerFn {
                ::leptos_workers::workers::FutureWorkerFn::new::<TestFutureWorker>()
            }
        );
        assert_eq_quoted(&expected, &ir.wasm_bindgen_func);

        let expected: ItemImpl = parse_quote!(
            impl ::leptos_workers::workers::FutureWorker for TestFutureWorker {
                fn run(request: Self::Request) -> ::leptos_workers::LocalBoxFuture<'static, Self::Response> {
                    async fn inner(mut request: TestRequest) -> TestResponse {
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
        assert_eq_quoted(&expected, &ir.impl_type_worker);

        let expected: ItemFn = parse_quote!(
            #[doc = r" function-level doc"]
            pub async fn future_worker(
                request: TestRequest,
            ) -> Result<TestResponse, ::leptos_workers::CreateWorkerError> {
                thread_local! {
                    static POOL: Result<::leptos_workers::executors::PoolExecutor<TestFutureWorker>, ::leptos_workers::CreateWorkerError> =
                        ::leptos_workers::executors::PoolExecutor::<TestFutureWorker>::new(2usize);
                }
                Ok(POOL
                    .with(move |pool| {
                        pool.as_ref()
                            .map_err(Clone::clone)
                            .and_then(|pool| pool.run(request))
                    })?
                    .1
                    .await)
            }
        );
        assert_eq_quoted(&expected, &ir.default_pool_func);
    }

    #[test]
    #[cfg(feature = "ssr")]
    fn produces_future_worker_ir_ssr() {
        let model = Model {
            worker_name: parse_quote!(TestFutureWorker),
            worker_type: WorkerType::Future(WorkerTypeFuture {
                request_pat: parse_quote!(request),
                request_type: parse_quote!(TestRequest),
                response_type: parse_quote!(TestResponse),
            }),
            visibility: parse_quote!(pub),
            function_attrs: vec![parse_quote!(
                /// function-level doc
            )],
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
        assert_eq_quoted(&expected, &ir.worker_struct);

        let expected: ItemImpl = parse_quote!(
            impl ::leptos_workers::workers::WebWorker for TestFutureWorker {
                type Request = TestRequest;
                type Response = TestResponse;
            }
        );
        assert_eq_quoted(&expected, &ir.impl_web_worker);

        let expected: ItemImpl = parse_quote!(
            impl ::leptos_workers::workers::WebWorkerPath for TestFutureWorker {
                fn path() -> &'static str {
                    "TestFutureWorker"
                }
            }
        );
        assert_eq_quoted(&expected, &ir.impl_web_worker_path);

        let expected: ItemFn = parse_quote!(
            #[::leptos_workers::wasm_bindgen]
            #[allow(non_snake_case)]
            pub fn WORKERS_FUTURE_TestFutureWorker() -> ::leptos_workers::workers::FutureWorkerFn {
                ::leptos_workers::workers::FutureWorkerFn::new::<TestFutureWorker>()
            }
        );
        assert_eq_quoted(&expected, &ir.wasm_bindgen_func);

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
        assert_eq_quoted(&expected, &ir.impl_type_worker);

        let expected: ItemFn = parse_quote!(
            #[doc = r" function-level doc"]
            pub async fn future_worker(
                request: TestRequest,
            ) -> Result<TestResponse, ::leptos_workers::CreateWorkerError> {
                let _ = request;
                if true {
                    panic!("Workers can't be constructed on the server. Try using `create_effect`, `create_local_resource`, `create_action` or `spawn_local`.")
                }
                thread_local! {
                    static POOL: Result<::leptos_workers::executors::PoolExecutor<TestFutureWorker>, ::leptos_workers::CreateWorkerError> =
                        ::leptos_workers::executors::PoolExecutor::<TestFutureWorker>::new(2usize);
                }
                Ok(POOL
                    .with(move |pool| {
                        pool.as_ref()
                            .map_err(Clone::clone)
                            .and_then(|pool| pool.run(request))
                    })?
                    .1
                    .await)
            }
        );
        assert_eq_quoted(&expected, &ir.default_pool_func);
    }
}
