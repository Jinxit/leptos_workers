#![warn(clippy::pedantic)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::too_many_arguments)]
// while API is still being solidified
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

use proc_macro2::{Ident, TokenStream};
use proc_macro_error::proc_macro_error;
use quote::{format_ident, quote, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Comma;
use syn::{
    FnArg, GenericArgument, ItemFn, PatType, PathArguments, ReturnType, Stmt, Type, TypeParamBound,
    Visibility,
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
    let visibility = function.vis;

    if let Some(request_response_pat_types) =
        get_response_request_pattern_types_from_channel_inputs(&inputs)
    {
        // ChannelWorker
        return channel_worker_impl(
            &worker_name,
            &visibility,
            &ident,
            request_response_pat_types,
            &statements,
        );
    }

    let request_pat_type = get_request_pat_type(&inputs)?;

    if let ReturnType::Default = output {
        // CallbackWorker
        callback_worker_impl(
            &worker_name,
            &visibility,
            &ident,
            &inputs,
            request_pat_type,
            &statements,
        )
    } else if let Some(response_type) = get_response_type_from_stream_output(&output) {
        // StreamWorker
        return stream_worker_impl(
            &worker_name,
            &visibility,
            &ident,
            &output,
            response_type,
            request_pat_type,
            &statements,
        );
    } else {
        // FutureWorker
        return future_worker_impl(
            &worker_name,
            &visibility,
            &ident,
            &output,
            request_pat_type,
            &statements,
        );
    }
}

fn channel_worker_impl(
    worker_name: &Ident,
    visibility: &Visibility,
    ident: &Ident,
    request_response_pat_types: RequestResponsePatTypes,
    statements: &[Stmt],
) -> syn::Result<TokenStream> {
    let fn_ident = format_ident!("WORKERS_CHANNEL_{}", &worker_name);
    let RequestResponsePatTypes {
        request_pat_type,
        request_type,
        response_pat_type,
        response_type,
    } = request_response_pat_types;
    let web_worker_part = web_worker_impl(
        worker_name,
        visibility,
        &request_type,
        &response_type.to_token_stream(),
    );

    let mut rx_response_type = request_pat_type.ty.clone();
    replace_generic_inner(&mut rx_response_type, response_type)?;
    let request_pat = request_pat_type.pat;
    let mut tx_request_type = response_pat_type.ty.clone();
    replace_generic_inner(&mut tx_request_type, request_type)?;
    let response_pat = response_pat_type.pat;

    let creation_error_msg = format!("worker creation failed for {worker_name}");

    Ok(quote! {
        #web_worker_part

        #[wasm_bindgen::prelude::wasm_bindgen]
        #[allow(non_snake_case)]
        pub fn #fn_ident() -> ::leptos_workers::workers::channel_worker::ChannelWorkerFn {
            ::leptos_workers::workers::channel_worker::ChannelWorkerFn::new::<#worker_name>()
        }

        impl ::leptos_workers::workers::channel_worker::ChannelWorker for #worker_name {
            fn channel(#request_pat: ::leptos_workers::Receiver<Self::Request>, #response_pat: ::leptos_workers::Sender<Self::Response>) -> ::leptos_workers::BoxFuture<'static, ()>  {
                Box::pin(async move {
                    #(#statements)*
                })
            }
        }

        #[allow(clippy::type_complexity)]
        #visibility async fn #ident() -> (#tx_request_type, #rx_response_type) {
            thread_local! {
                static POOL: ::leptos_workers::executors::PoolExecutor<#worker_name> =
                    ::leptos_workers::executors::PoolExecutor::<#worker_name>::new(1).unwrap();
            }
            let (_, tx, rx) = POOL.with(move |pool| pool.channel()).expect(#creation_error_msg);
            (tx, rx)
        }
    })
}

fn stream_worker_impl(
    worker_name: &Ident,
    visibility: &Visibility,
    ident: &Ident,
    output: &ReturnType,
    response_type: &Type,
    request_pat_type: &PatType,
    statements: &Vec<Stmt>,
) -> syn::Result<TokenStream> {
    let fn_ident = format_ident!("WORKERS_STREAM_{}", &worker_name);
    let web_worker_part = web_worker_impl(
        worker_name,
        visibility,
        &request_pat_type.ty,
        &response_type.to_token_stream(),
    );

    let request_pat = &request_pat_type.pat;
    let request_type = &request_pat_type.ty;

    let stream_response_impl = get_output_type(output)
        .ok_or_else(|| syn::Error::new(output.span(), "couldn't get type from Stream output"))?;
    let stream_response_ident = get_impl_ident(stream_response_impl)
        .ok_or_else(|| syn::Error::new(output.span(), "couldn't get impl type from Stream output"))?
        .clone();

    let creation_error_msg = format!("worker creation failed for {worker_name}");

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

        #visibility async fn #ident(
            #request_pat: &#request_type,
        ) -> impl #stream_response_ident<Item = #response_type> {
            thread_local! {
                static POOL: ::leptos_workers::executors::PoolExecutor<#worker_name> =
                    ::leptos_workers::executors::PoolExecutor::<#worker_name>::new(2).unwrap();
            }
            POOL.with(move |pool| pool.stream(#request_pat)).expect(#creation_error_msg).1
        }
    })
}

fn callback_worker_impl(
    worker_name: &Ident,
    visibility: &Visibility,
    ident: &Ident,
    inputs: &Punctuated<FnArg, Comma>,
    request_pat_type: &PatType,
    statements: &Vec<Stmt>,
) -> syn::Result<TokenStream> {
    let fn_ident = format_ident!("WORKERS_CALLBACK_{}", &worker_name);
    let callback_pat_type = get_callback_pat_type(inputs)?;
    let callback_pat = &callback_pat_type.pat;
    let response_type =
        get_response_type_from_callback(&callback_pat_type.ty).ok_or_else(move || {
            syn::Error::new(callback_pat_type.ty.span(), "Invalid type for callback")
        })?;
    let web_worker_part = web_worker_impl(
        worker_name,
        visibility,
        &request_pat_type.ty,
        &response_type.to_token_stream(),
    );

    let request_pat = &request_pat_type.pat;
    let request_type = &request_pat_type.ty;

    let creation_error_msg = format!("worker creation failed for {worker_name}");

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

        #visibility async fn #ident(
            #request_pat: #request_type,
            #callback_pat: impl Fn(#response_type) + 'static,
        ) {
            thread_local! {
                static POOL: ::leptos_workers::executors::PoolExecutor<#worker_name> =
                    ::leptos_workers::executors::PoolExecutor::<#worker_name>::new(2).expect(#creation_error_msg);
            }
            POOL.with(move |pool| pool.stream_callback(#request_pat, Box::new(#callback_pat))).expect(#creation_error_msg).1.await
        }
    })
}

fn future_worker_impl(
    worker_name: &Ident,
    visibility: &Visibility,
    ident: &Ident,
    output: &ReturnType,
    request_pat_type: &PatType,
    statements: &Vec<Stmt>,
) -> syn::Result<TokenStream> {
    let fn_ident = format_ident!("WORKERS_FUTURE_{}", &worker_name);
    let response_type = get_response_type_from_output(output)?;
    let web_worker_part = web_worker_impl(
        worker_name,
        visibility,
        &request_pat_type.ty,
        &response_type.to_token_stream(),
    );

    let request_pat = &request_pat_type.pat;
    let request_type = &request_pat_type.ty;

    let creation_error_msg = format!("worker creation failed for {worker_name}");

    Ok(quote! {
        #web_worker_part

        #[wasm_bindgen::prelude::wasm_bindgen]
        #[allow(non_snake_case)]
        pub fn #fn_ident() -> ::leptos_workers::workers::future_worker::FutureWorkerFn {
            ::leptos_workers::workers::future_worker::FutureWorkerFn::new::<#worker_name>()
        }

        impl ::leptos_workers::workers::future_worker::FutureWorker for #worker_name {
            fn run(#request_pat: Self::Request) -> ::leptos_workers::BoxFuture<'static, Self::Response> {
                Box::pin(async move {
                    #(#statements)*
                })
            }
        }

        #visibility async fn #ident(
            #request_pat: #request_type,
        ) -> #response_type {
            thread_local! {
                static POOL: ::leptos_workers::executors::PoolExecutor<#worker_name> =
                    ::leptos_workers::executors::PoolExecutor::<#worker_name>::new(2).expect(#creation_error_msg);
            }
            POOL.with(move |pool| pool.run(#request_pat)).expect(#creation_error_msg).1.await
        }
    })
}

fn web_worker_impl(
    worker_name: &Ident,
    visibility: &Visibility,
    request_type: &Type,
    response_type: &TokenStream,
) -> TokenStream {
    quote! {
        #[derive(Debug, Clone)]
        #visibility struct #worker_name;

        impl ::leptos_workers::workers::web_worker::WebWorker for #worker_name {
            type Request = #request_type;
            type Response = #response_type;
        }

        impl ::leptos_workers::workers::web_worker::WebWorkerPath for #worker_name {
            fn path() -> &'static str {
                stringify!(#worker_name)
            }
        }
    }
}

fn get_impl_ident(ty: &Type) -> Option<&Ident> {
    if let Type::ImplTrait(impl_trait) = ty {
        let segment = impl_trait
            .bounds
            .iter()
            .filter_map(|b| match b {
                TypeParamBound::Trait(trait_bound) => Some(trait_bound),
                _ => None,
            })
            .flat_map(|tb| &tb.path.segments)
            .find(|s| matches!(&s.arguments, PathArguments::AngleBracketed(_)))?;

        Some(&segment.ident)
    } else {
        None
    }
}

fn replace_generic_inner(generic: &mut Type, new_type: Type) -> syn::Result<()> {
    if let Type::Path(type_paren) = generic {
        if let Some(generic_args) =
            type_paren
                .path
                .segments
                .iter_mut()
                .find_map(|s| match &mut s.arguments {
                    PathArguments::AngleBracketed(args) => Some(args),
                    _ => None,
                })
        {
            if let Some(non_lifetime_arg) = generic_args.args.iter_mut().find_map(|a| match a {
                GenericArgument::Type(ty) => Some(ty),
                _ => None,
            }) {
                *non_lifetime_arg = new_type;
                return Ok(());
            }
        }
    }

    Err(syn::Error::new(
        generic.span(),
        "tried to replace the inner type of a non-generic type",
    ))
}

struct RequestResponsePatTypes {
    request_pat_type: PatType,
    request_type: Type,
    response_pat_type: PatType,
    response_type: Type,
}

fn get_response_request_pattern_types_from_channel_inputs(
    inputs: &Punctuated<FnArg, Comma>,
) -> Option<RequestResponsePatTypes> {
    let request_pat_type = &get_pat_type_from_typed_fn_arg(inputs.first()?)?;
    let request_type = get_generic_inner_type(&request_pat_type.ty)?;
    let response_pat_type = &get_pat_type_from_typed_fn_arg(inputs.last()?)?;
    let response_type = get_generic_inner_type(&response_pat_type.ty)?;
    Some(RequestResponsePatTypes {
        request_pat_type: (*request_pat_type).clone(),
        request_type: request_type.clone(),
        response_pat_type: (*response_pat_type).clone(),
        response_type: response_type.clone(),
    })
}
fn get_pat_type_from_typed_fn_arg(fn_arg: &FnArg) -> Option<&PatType> {
    if let FnArg::Typed(pat_type) = fn_arg {
        Some(pat_type)
    } else {
        None
    }
}

fn get_generic_inner_type(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Path(path) => {
            let generic_args = path.path.segments.iter().find_map(|s| match &s.arguments {
                PathArguments::AngleBracketed(args) => Some(args),
                _ => None,
            })?;
            let first_typed = generic_args.args.iter().find_map(|a| match &a {
                GenericArgument::Type(ty) => Some(ty),
                _ => None,
            })?;

            Some(first_typed)
        }
        Type::ImplTrait(impl_trait) => {
            let generic_args = impl_trait
                .bounds
                .iter()
                .filter_map(|b| match &b {
                    TypeParamBound::Trait(trait_bound) => Some(trait_bound),
                    _ => None,
                })
                .flat_map(|tb| &tb.path.segments)
                .find_map(|s| match &s.arguments {
                    PathArguments::AngleBracketed(args) => Some(args),
                    _ => None,
                })?;
            let first_typed = generic_args.args.iter().find_map(|a| match &a {
                GenericArgument::Type(ty) => Some(ty),
                _ => None,
            })?;

            Some(first_typed)
        }
        _ => None,
    }
}

fn get_response_type_from_callback(ty: &Type) -> Option<TokenStream> {
    match ty {
        Type::ImplTrait(impl_trait) => {
            let generic_args = impl_trait
                .bounds
                .iter()
                .filter_map(|b| match &b {
                    TypeParamBound::Trait(trait_bound) => Some(trait_bound),
                    _ => None,
                })
                .flat_map(|tb| &tb.path.segments)
                .find_map(|s| match &s.arguments {
                    PathArguments::Parenthesized(args) => Some(args),
                    _ => None,
                })?;
            let first_path = generic_args.inputs.iter().find_map(|a| match &a {
                Type::Path(path) => Some(path),
                _ => None,
            })?;

            Some(first_path.to_token_stream())
        }
        _ => None,
    }
}

fn get_output_type(output: &ReturnType) -> Option<&Type> {
    if let ReturnType::Type(_, ty) = output {
        Some(ty)
    } else {
        None
    }
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
