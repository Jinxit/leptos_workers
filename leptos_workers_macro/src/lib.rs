#![warn(clippy::pedantic)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::too_many_arguments)]
// while API is still being solidified
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

use proc_macro2::{Ident, TokenStream};
use proc_macro_error::{abort_call_site, proc_macro_error};
use quote::{format_ident, quote, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Comma;
use syn::{
    AngleBracketedGenericArguments, FnArg, GenericArgument, ItemFn, ParenthesizedGenericArguments,
    Pat, PatType, PathArguments, PathSegment, ReturnType, Stmt, TraitBound, Type, TypeParamBound,
    TypeTraitObject,
};

extern crate proc_macro2;

#[proc_macro_error]
#[proc_macro_attribute]
pub fn worker(
    args: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    match worker_impl(args.into(), item.into()) {
        Err(e) => e.to_compile_error().into(),
        Ok(s) => s.to_token_stream().into(),
    }
}

fn worker_impl(args: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let WorkerName { worker_name } = syn::parse2::<WorkerName>(args)?;
    let function = syn::parse2::<ItemFn>(item)?;

    let ident = function.sig.ident;
    let inputs = function.sig.inputs;
    let output = function.sig.output;
    let statements = function.block.stmts;

    if let Some((response_type, request_type)) =
        get_response_request_types_from_channel_inputs(&inputs)
    {
        if let Some((response_pat, request_pat)) =
            get_response_request_patterns_from_channel_inputs(&inputs)
        {
            // ChannelWorker
            return channel_worker_impl(
                &worker_name,
                &ident,
                response_pat,
                response_type,
                request_pat,
                request_type,
                &statements,
            );
        }
    }

    let request_pat_type = get_request_pat_type(&inputs)?;
    let request_type = &request_pat_type.ty;
    let request_pat = &request_pat_type.pat;

    if function.sig.asyncness.is_some() {
        // FutureWorker
        return future_worker_impl(
            &worker_name,
            &ident,
            &output,
            request_type,
            request_pat,
            &statements,
        );
    } else if let ReturnType::Default = output {
        // CallbackWorker
        return callback_worker_impl(
            &worker_name,
            &ident,
            &inputs,
            request_type,
            request_pat,
            &statements,
        );
    } else if let Some(response_type) = get_response_type_from_stream_output(&output) {
        // StreamWorker
        return stream_worker_impl(
            &worker_name,
            &ident,
            response_type,
            request_type,
            request_pat,
            &statements,
        );
    }

    abort_call_site!(
        r#"No valid Worker type for function signature. Expected one of:
        async fn future_worker(request: Request) -> Response
        fn stream_worker(request: Request) -> BoxStream<'static, Response>
        fn callback_worker(request: Request, callback: Box<dyn Fn(Response)>)
        fn channel_worker(tx: flume::Sender<Response>, rx: flume::Receiver<Request>)
    "#
    )
}

fn channel_worker_impl(
    worker_name: &Ident,
    ident: &Ident,
    response_pat: &Pat,
    response_type: &Type,
    request_pat: &Pat,
    request_type: &Type,
    statements: &[Stmt],
) -> syn::Result<TokenStream> {
    let fn_ident = format_ident!("WORKERS_CHANNEL_{}", &worker_name);
    let web_worker_part = web_worker_impl(worker_name, request_type, response_type);

    Ok(quote! {
        #web_worker_part

        #[wasm_bindgen::prelude::wasm_bindgen]
        #[allow(non_snake_case)]
        pub fn #fn_ident() -> ::leptos_workers::workers::channel_worker::ChannelWorkerFn {
            ::leptos_workers::workers::channel_worker::ChannelWorkerFn::new::<#worker_name>()
        }

        impl ::leptos_workers::workers::channel_worker::ChannelWorker for #worker_name {
            fn channel(#response_pat: ::leptos_workers::Sender<Self::Response>, #request_pat: ::leptos_workers::Receiver<Self::Request>) -> ::leptos_workers::BoxFuture<'static, ()>  {
                Box::pin(async move {
                    #(#statements)*
                })
            }
        }

        #[allow(clippy::type_complexity)]
        pub fn #ident() -> Result<
            (
                ::leptos_workers::executors::AbortHandle<#worker_name>,
                ::leptos_workers::Sender<#request_type>,
                ::leptos_workers::Receiver<#response_type>,
            ),
            ::leptos_workers::CreateWorkerError,
        > {
            thread_local! {
                static POOL: ::leptos_workers::executors::PoolExecutor<#worker_name> =
                    ::leptos_workers::executors::PoolExecutor::<#worker_name>::new(1).unwrap();
            }
            POOL.with(move |pool| pool.channel())
        }
    })
}

fn stream_worker_impl(
    worker_name: &Ident,
    ident: &Ident,
    response_type: &Type,
    request_type: &Type,
    request_pat: &Pat,
    statements: &Vec<Stmt>,
) -> syn::Result<TokenStream> {
    let fn_ident = format_ident!("WORKERS_STREAM_{}", &worker_name);
    let web_worker_part = web_worker_impl(worker_name, request_type, response_type);

    Ok(quote! {
        #web_worker_part

        #[wasm_bindgen::prelude::wasm_bindgen]
        #[allow(non_snake_case)]
        pub fn #fn_ident() -> ::leptos_workers::workers::stream_worker::StreamWorkerFn {
            ::leptos_workers::workers::stream_worker::StreamWorkerFn::new::<#worker_name>()
        }

        impl ::leptos_workers::workers::stream_worker::StreamWorker for #worker_name {
            fn stream(#request_pat: Self::Request) -> ::leptos_workers::BoxStream<'static, Self::Response> {
                #(#statements)*
            }
        }

        pub fn #ident(
            #request_pat: &#request_type,
        ) -> Result<
            (
                ::leptos_workers::executors::AbortHandle<#worker_name>,
                impl ::leptos_workers::Stream<Item = #response_type>,
            ),
            ::leptos_workers::CreateWorkerError,
        > {
            thread_local! {
                static POOL: ::leptos_workers::executors::PoolExecutor<#worker_name> =
                    ::leptos_workers::executors::PoolExecutor::<#worker_name>::new(2).unwrap();
            }
            POOL.with(move |pool| pool.stream(#request_pat))
        }
    })
}

fn callback_worker_impl(
    worker_name: &Ident,
    ident: &Ident,
    inputs: &Punctuated<FnArg, Comma>,
    request_type: &Type,
    request_pat: &Pat,
    statements: &Vec<Stmt>,
) -> syn::Result<TokenStream> {
    let fn_ident = format_ident!("WORKERS_CALLBACK_{}", &worker_name);
    let callback_pat_type = get_callback_pat_type(inputs)?;
    let callback_pat = &callback_pat_type.pat;
    let response_type = get_response_type_from_callback(&callback_pat_type.ty)?;
    let web_worker_part = web_worker_impl(worker_name, request_type, response_type);

    Ok(quote! {
        #web_worker_part

        #[wasm_bindgen::prelude::wasm_bindgen]
        #[allow(non_snake_case)]
        pub fn #fn_ident() -> ::leptos_workers::workers::callback_worker::CallbackWorkerFn {
            ::leptos_workers::workers::callback_worker::CallbackWorkerFn::new::<#worker_name>()
        }

        impl ::leptos_workers::workers::callback_worker::CallbackWorker for #worker_name {
            fn stream_callback(#request_pat: Self::Request, #callback_pat: Box<dyn Fn(Self::Response)>) {
                #(#statements)*
            }
        }

        pub fn #ident(
            #request_pat: #request_type,
            #callback_pat: impl Fn(#response_type) + 'static,
        ) -> Result<
            (
                ::leptos_workers::executors::AbortHandle<#worker_name>,
                impl ::std::future::Future<Output = ()>,
            ),
            ::leptos_workers::CreateWorkerError,
        > {
            thread_local! {
                static POOL: ::leptos_workers::executors::PoolExecutor<#worker_name> =
                    ::leptos_workers::executors::PoolExecutor::<#worker_name>::new(2).unwrap();
            }
            POOL.with(move |pool| pool.stream_callback(#request_pat, Box::new(#callback_pat)))
        }
    })
}

fn future_worker_impl(
    worker_name: &Ident,
    ident: &Ident,
    output: &ReturnType,
    request_type: &Type,
    request_pat: &Pat,
    statements: &Vec<Stmt>,
) -> syn::Result<TokenStream> {
    let fn_ident = format_ident!("WORKERS_FUTURE_{}", &worker_name);
    let response_type = get_response_type_from_output(output)?;
    let web_worker_part = web_worker_impl(worker_name, request_type, response_type);

    Ok(quote! {
        #web_worker_part

        #[wasm_bindgen::prelude::wasm_bindgen]
        #[allow(non_snake_case)]
        pub fn #fn_ident() -> ::leptos_workers::workers::future_worker::FutureWorkerFn {
            ::leptos_workers::workers::future_worker::FutureWorkerFn::new::<#worker_name>()
        }

        impl ::leptos_workers::workers::future_worker::FutureWorker for #worker_name {
            fn run(#request_pat: Self::Request) -> ::leptos_workers::BoxFuture<'static, Self::Response> {
                #(#statements)*
            }
        }

        pub fn #ident(
            #request_pat: #request_type,
        ) -> Result<
            (
                ::leptos_workers::executors::AbortHandle<#worker_name>,
                impl ::std::future::Future<Output = #response_type>,
            ),
            ::leptos_workers::CreateWorkerError,
        > {
            thread_local! {
                static POOL: ::leptos_workers::executors::PoolExecutor<#worker_name> =
                    ::leptos_workers::executors::PoolExecutor::<#worker_name>::new(2).unwrap();
            }
            POOL.with(move |pool| pool.run(#request_pat))
        }
    })
}

fn web_worker_impl(worker_name: &Ident, request_type: &Type, response_type: &Type) -> TokenStream {
    quote! {
        #[derive(Debug, Clone)]
        pub struct #worker_name;

        impl ::leptos_workers::workers::web_worker::WebWorker for #worker_name {
            type Request = #request_type;
            type Response = #response_type;
        }

        impl ::leptos_workers::workers::web_worker::WebWorkerPath for #worker_name {
            fn path() -> &'static str {
                /*
                let path_hash: u64 = ::leptos_workers::xxhash_rust::const_xxh64::xxh64(
                    concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        ":",
                        file!(),
                        ":",
                        line!(),
                        ":",
                        column!()
                    )
                    .as_bytes(),
                    0,
                );
                std::format!(
                    "{}_{}"
                    stringify!(#worker_name),
                    path_hash
                )*/
                stringify!(#worker_name)
            }
        }
    }
}

fn get_response_request_patterns_from_channel_inputs(
    inputs: &Punctuated<FnArg, Comma>,
) -> Option<(&Pat, &Pat)> {
    let response_pat = &get_pat_type_from_typed_fn_arg(inputs.first()?)?.pat;
    let request_pat = &get_pat_type_from_typed_fn_arg(inputs.last()?)?.pat;
    Some((response_pat, request_pat))
}

fn get_response_request_types_from_channel_inputs(
    inputs: &Punctuated<FnArg, Comma>,
) -> Option<(&Type, &Type)> {
    let response_type =
        get_generic_inner_type(&get_pat_type_from_typed_fn_arg(inputs.first()?)?.ty)?;
    let request_type = get_generic_inner_type(&get_pat_type_from_typed_fn_arg(inputs.last()?)?.ty)?;
    Some((response_type, request_type))
}

fn get_pat_type_from_typed_fn_arg(fn_arg: &FnArg) -> Option<&PatType> {
    if let FnArg::Typed(pat_type) = fn_arg {
        Some(pat_type)
    } else {
        None
    }
}

fn get_generic_inner_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(path) = ty {
        let generic_args = path.path.segments.iter().find_map(|s| match &s.arguments {
            PathArguments::AngleBracketed(args) => Some(args),
            _ => None,
        })?;
        let first_typed = generic_args.args.iter().find_map(|a| match &a {
            GenericArgument::Type(ty) => Some(ty),
            _ => None,
        })?;

        Some(first_typed)
    } else {
        None
    }
}

fn get_response_type_from_callback(ty: &Type) -> syn::Result<&Type> {
    if let Type::Path(path) = ty {
        if let Some(segment) = path.path.segments.iter().find(|s| s.ident == "Box") {
            if let PathArguments::AngleBracketed(AngleBracketedGenericArguments { args, .. }) =
                &segment.arguments
            {
                if let Some(GenericArgument::Type(Type::TraitObject(TypeTraitObject {
                    bounds,
                    ..
                }))) = args.first()
                {
                    if let Some(TypeParamBound::Trait(TraitBound { path, .. })) = bounds.first() {
                        if let Some(PathSegment {
                            arguments:
                                PathArguments::Parenthesized(ParenthesizedGenericArguments {
                                    inputs,
                                    ..
                                }),
                            ..
                        }) = path.segments.iter().find(|s| s.ident == "Fn")
                        {
                            if let Some(input) = inputs.first() {
                                return Ok(input);
                            }
                        }
                    }
                }
            }
        }
    }

    Err(syn::Error::new(ty.span(), "Invalid type for callback"))
}

fn get_response_type_from_stream_output(output: &ReturnType) -> Option<&Type> {
    if let ReturnType::Type(_, ty) = output {
        Some(get_generic_inner_type(ty)?)
    } else {
        None
    }
}

fn get_response_type_from_output(output: &ReturnType) -> syn::Result<&Type> {
    if let ReturnType::Type(_, ty) = output {
        Ok(ty)
    } else {
        Err(syn::Error::new(output.span(), "Response type not found"))
    }
}

fn get_request_pat_type(inputs: &Punctuated<FnArg, Comma>) -> syn::Result<&PatType> {
    if let Some(FnArg::Typed(ty)) = inputs.first() {
        Ok(ty)
    } else {
        Err(syn::Error::new(inputs.span(), "Request argument not found"))
    }
}

fn get_callback_pat_type(inputs: &Punctuated<FnArg, Comma>) -> syn::Result<&PatType> {
    if let Some(FnArg::Typed(ty)) = inputs.last() {
        Ok(ty)
    } else {
        Err(syn::Error::new(
            inputs.span(),
            "Callback argument not found",
        ))
    }
}

struct WorkerName {
    worker_name: Ident,
}

impl Parse for WorkerName {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let worker_name = input.parse()?;

        Ok(Self { worker_name })
    }
}
