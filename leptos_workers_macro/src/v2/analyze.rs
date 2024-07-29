use crate::v2::parse::Ast;
use crate::v2::pattern_match_hole::pattern_match_holes;
use indoc::indoc;
use proc_macro2::Ident;
use proc_macro_error::abort;
use quote::quote;
use syn::spanned::Spanned;
use syn::{
    parse_quote, parse_quote_spanned, Attribute, FnArg, Generics, Pat, ReturnType, Signature, Stmt,
    Type, Visibility,
};

pub fn analyze(ast: Ast) -> Model {
    let worker_name = ast.worker_name;
    let worker_type = analyze_worker_type(&ast.item_fn.sig);
    let visibility = ast.item_fn.vis;
    let function_name = ast.item_fn.sig.ident;
    let function_body = ast.item_fn.block.stmts;
    let function_attrs = ast.item_fn.attrs;

    Model {
        worker_name,
        worker_type,
        visibility,
        function_attrs,
        function_name,
        function_body,
    }
}

const VALID_SIGNATURES: &str = indoc! { r"
    try one of the following:
    Callback:
        [pub] [async] fn worker(req: Request, callback: impl Fn(Response))
    Channel:
        [pub] [async] fn worker(rx: leptos_workers::Receiver<Request>, tx: leptos_workers::Sender<Response>)
        or with an initialisation parameter:
        [pub] [async] fn worker(init: Init, rx: leptos_workers::Receiver<Request>, tx: leptos_workers::Sender<Response>)
    Future:
        [pub] [async] fn worker(req: Request) -> Response
    Stream:
        [pub] [async] fn worker(req: Request) -> impl leptos_workers::Stream<Item = Response>
" };

#[allow(clippy::too_many_lines)]
fn analyze_worker_type(sig: &Signature) -> WorkerType {
    // this needs the full Option::None to properly pattern match
    #[allow(unused_qualifications)]
    let Signature {
        unsafety: Option::None,
        abi: Option::None,
        generics: Generics {
            lt_token: Option::None,
            ..
        },
        inputs,
        variadic: Option::None,
        output,
        ..
    } = &sig
    else {
        abort!(sig, "couldn't match worker type"; help = VALID_SIGNATURES)
    };

    let inputs = inputs.iter().collect::<Vec<_>>();
    if let &[FnArg::Typed(first), FnArg::Typed(second), FnArg::Typed(third)] = inputs.as_slice() {
        // channel case WITH init parameter:

        let receiver_opt = pattern_match_holes(
            &[
                quote!(leptos_workers::Receiver<@>),
                quote!(flume::Receiver<@>),
                quote!(Receiver<@>),
            ],
            &second.ty,
        );
        let sender_opt = pattern_match_holes(
            &[
                quote!(leptos_workers::Sender<@>),
                quote!(flume::Sender<@>),
                quote!(Sender<@>),
            ],
            &third.ty,
        );
        if let Some((second_tokens, third_tokens)) = receiver_opt.zip(sender_opt) {
            WorkerType::Channel(WorkerTypeChannel {
                init_pat_and_type: Some((*first.pat.clone(), *first.ty.clone())),
                receiver_pat: *second.pat.clone(),
                receiver_type: *second.ty.clone(),
                request_type: parse_quote_spanned!(second.span()=> #second_tokens),
                sender_pat: *third.pat.clone(),
                sender_type: *third.ty.clone(),
                response_type: parse_quote_spanned!(third.span()=> #third_tokens),
            })
        } else {
            abort!(sig, "couldn't match worker type"; help = VALID_SIGNATURES)
        }
    } else if let &[FnArg::Typed(first), FnArg::Typed(second)] = inputs.as_slice() {
        // channel case WITHOUT init parameter:

        let receiver_opt = pattern_match_holes(
            &[
                quote!(leptos_workers::Receiver<@>),
                quote!(flume::Receiver<@>),
                quote!(Receiver<@>),
            ],
            &first.ty,
        );
        let sender_opt = pattern_match_holes(
            &[
                quote!(leptos_workers::Sender<@>),
                quote!(flume::Sender<@>),
                quote!(Sender<@>),
            ],
            &second.ty,
        );
        if let Some((first_tokens, second_tokens)) = receiver_opt.zip(sender_opt) {
            // Empty () is used as the init type
            return WorkerType::Channel(WorkerTypeChannel {
                init_pat_and_type: None,
                receiver_pat: *first.pat.clone(),
                receiver_type: *first.ty.clone(),
                request_type: parse_quote_spanned!(first.span()=> #first_tokens),
                sender_pat: *second.pat.clone(),
                sender_type: *second.ty.clone(),
                response_type: parse_quote_spanned!(second.span()=> #second_tokens),
            });
        }

        // callback case:

        if let Some(tokens) = pattern_match_holes(&[quote!(impl Fn(@))], &second.ty) {
            WorkerType::Callback(WorkerTypeCallback {
                request_pat: *first.pat.clone(),
                request_type: *first.ty.clone(),
                callback_pat: *second.pat.clone(),
                response_type: parse_quote_spanned!(second.span()=> #tokens),
            })
        } else {
            abort!(sig, "couldn't match worker type"; help = VALID_SIGNATURES)
        }
    } else if let &[FnArg::Typed(input)] = inputs.as_slice() {
        let ReturnType::Type(_, return_type) = output else {
            abort!(sig, "couldn't match worker type"; help = VALID_SIGNATURES)
        };
        if let Some(tokens) = pattern_match_holes(
            &[
                quote!(impl leptos_workers::Stream<Item = @>),
                quote!(impl futures::stream::Stream<Item = @>),
                quote!(impl Stream<Item = @>),
            ],
            return_type,
        ) {
            WorkerType::Stream(WorkerTypeStream {
                request_pat: *input.pat.clone(),
                request_type: *input.ty.clone(),
                response_type: parse_quote_spanned!(output.span()=> #tokens),
                return_type: *return_type.clone(),
            })
        } else if let Type::ImplTrait(_) = &*return_type.clone() {
            abort!(return_type, "couldn't match worker type"; help = VALID_SIGNATURES)
        } else {
            WorkerType::Future(WorkerTypeFuture {
                request_pat: *input.pat.clone(),
                request_type: *input.ty.clone(),
                response_type: *return_type.clone(),
            })
        }
    } else {
        abort!(sig, "invalid number of arguments to worker, must be 1, 2 or 3"; help = VALID_SIGNATURES)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WorkerType {
    Callback(WorkerTypeCallback),
    Channel(WorkerTypeChannel),
    Future(WorkerTypeFuture),
    Stream(WorkerTypeStream),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorkerTypeCallback {
    pub request_pat: Pat,
    pub request_type: Type,
    pub callback_pat: Pat,
    pub response_type: Type,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorkerTypeChannel {
    // Init is an optional parameter.
    pub init_pat_and_type: Option<(Pat, Type)>,
    pub receiver_pat: Pat,
    pub receiver_type: Type,
    pub request_type: Type,
    pub sender_pat: Pat,
    pub sender_type: Type,
    pub response_type: Type,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorkerTypeFuture {
    pub request_pat: Pat,
    pub request_type: Type,
    pub response_type: Type,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorkerTypeStream {
    pub request_pat: Pat,
    pub request_type: Type,
    pub response_type: Type,
    pub return_type: Type,
}

impl WorkerType {
    pub fn wasm_function_prefix(&self) -> &'static str {
        match self {
            WorkerType::Callback(_) => "WORKERS_CALLBACK",
            WorkerType::Channel(_) => "WORKERS_CHANNEL",
            WorkerType::Future(_) => "WORKERS_FUTURE",
            WorkerType::Stream(_) => "WORKERS_STREAM",
        }
    }

    pub fn worker_fn_type(&self) -> Type {
        match self {
            WorkerType::Callback(_) => parse_quote!(CallbackWorkerFn),
            WorkerType::Channel(_) => parse_quote!(ChannelWorkerFn),
            WorkerType::Future(_) => parse_quote!(FutureWorkerFn),
            WorkerType::Stream(_) => parse_quote!(StreamWorkerFn),
        }
    }

    pub fn default_pool_size(&self) -> usize {
        match self {
            WorkerType::Channel(_) => 1,
            WorkerType::Future(_) | WorkerType::Callback(_) | WorkerType::Stream(_) => 2,
        }
    }

    pub fn request_type(&self) -> &Type {
        match self {
            WorkerType::Callback(WorkerTypeCallback { request_type, .. })
            | WorkerType::Channel(WorkerTypeChannel { request_type, .. })
            | WorkerType::Future(WorkerTypeFuture { request_type, .. })
            | WorkerType::Stream(WorkerTypeStream { request_type, .. }) => request_type,
        }
    }

    pub fn response_type(&self) -> &Type {
        match self {
            WorkerType::Callback(WorkerTypeCallback { response_type, .. })
            | WorkerType::Channel(WorkerTypeChannel { response_type, .. })
            | WorkerType::Future(WorkerTypeFuture { response_type, .. })
            | WorkerType::Stream(WorkerTypeStream { response_type, .. }) => response_type,
        }
    }
}

pub struct Model {
    pub worker_name: Ident,
    pub worker_type: WorkerType,
    pub visibility: Visibility,
    pub function_attrs: Vec<Attribute>,
    pub function_name: Ident,
    pub function_body: Vec<Stmt>,
}

#[allow(unused_imports)]
mod tests {
    use super::*;
    use crate::v2::analyze;
    use crate::v2::analyze::WorkerType;
    use pretty_assertions::assert_eq;
    use syn::parse_quote;

    #[test]
    fn can_extract_future_worker_model() {
        let model = analyze::analyze(Ast {
            worker_name: parse_quote!(Name),
            item_fn: parse_quote!(
                /// function-level doc
                async fn future_worker(
                    /// parameter-level doc
                    request: TestRequest,
                ) -> TestResponse {
                    statement1;
                    let statement2 = 0;
                    TestResponse
                }
            ),
        });

        let expected: Ident = parse_quote!(Name);
        assert_eq!(expected, model.worker_name);
        let expected: WorkerType = WorkerType::Future(WorkerTypeFuture {
            request_pat: parse_quote!(request),
            request_type: parse_quote!(TestRequest),
            response_type: parse_quote!(TestResponse),
        });
        assert_eq!(expected, model.worker_type);
        let expected: Ident = parse_quote!(future_worker);
        assert_eq!(expected, model.function_name);
        let expected: Vec<Stmt> = parse_quote!(
            statement1;
            let statement2 = 0;
            TestResponse
        );
        assert_eq!(expected, model.function_body);
    }
}
