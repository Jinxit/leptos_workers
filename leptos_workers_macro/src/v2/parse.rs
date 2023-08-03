use proc_macro2::{Ident, TokenStream};
use proc_macro_error::abort;
use syn::parse::{Parse, ParseStream};
use syn::{Item, ItemFn};

pub struct Ast {
    pub worker_name: Ident,
    pub item_fn: ItemFn,
}

pub fn parse(args: TokenStream, item: TokenStream) -> Ast {
    if args.is_empty() {
        abort!(
            args,
            "worker name missing";
            help = "try `#[worker(YourNameHere)]`"
        )
    }

    let worker_name = match syn::parse2::<Ident>(args.clone()) {
        Ok(ident) => ident,
        Err(_) => abort!(args, format!("invalid worker name: `{args}`")),
    };

    let item_fn = match syn::parse2::<Item>(item) {
        Ok(Item::Fn(item)) => item,
        Ok(item) => {
            abort!(item, "`#[worker]` can only be used on functions")
        }
        Err(_) => unreachable!(),
    };

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

    #[test]
    fn valid_syntax() {
        parse(
            quote!(Name),
            quote!(
                async fn future_worker(req: TestRequest) -> TestResponse {}
            ),
        );
    }
}
