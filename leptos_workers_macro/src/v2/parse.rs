use convert_case::{Case, Casing};
use proc_macro2::{Ident, TokenStream};
use proc_macro_error2::abort;
use quote::format_ident;
use syn::parse::{Parse, ParseStream};
use syn::{Item, ItemFn};

pub struct Ast {
    pub worker_name: Ident,
    pub item_fn: ItemFn,
}

pub fn parse(args: &TokenStream, item: TokenStream) -> Ast {
    let item_fn = match syn::parse2::<Item>(item) {
        Ok(Item::Fn(item)) => item,
        Ok(item) => {
            abort!(item, "`#[worker]` can only be used on functions")
        }
        Err(_) => unreachable!(),
    };

    let worker_name = syn::parse2::<Option<Ident>>(args.clone())
        .unwrap_or_else(|_| abort!(args, format!("invalid worker name: `{args}`")))
        .unwrap_or_else(|| {
            let worker_name = item_fn.sig.ident.to_string().to_case(Case::Pascal);
            format_ident!("{worker_name}")
        });

    Ast {
        worker_name,
        item_fn,
    }
}

impl Parse for Ast {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            worker_name: input.parse()?,
            item_fn: input.parse()?,
        })
    }
}

#[allow(unused_imports)]
mod tests {
    use super::parse;
    use quote::quote;
    use syn::{parse_quote, Ident};

    #[test]
    fn valid_syntax() {
        parse(
            &quote!(Name),
            quote!(
                async fn future_worker(req: TestRequest) -> TestResponse {}
            ),
        );
    }

    #[test]
    fn valid_syntax_no_name() {
        let ast = parse(
            &quote!(),
            quote!(
                async fn future_worker(req: TestRequest) -> TestResponse {}
            ),
        );
        let expected: Ident = parse_quote!(FutureWorker);
        assert_eq!(ast.worker_name, expected);
    }
}
