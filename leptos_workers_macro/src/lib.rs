#![warn(clippy::pedantic)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::too_many_arguments)]

mod v2;

use crate::v2::analyze::analyze;
use crate::v2::codegen::codegen;
use crate::v2::lower::lower;
use crate::v2::parse::parse;
use proc_macro_error::proc_macro_error;

extern crate proc_macro2;

#[proc_macro_error]
#[proc_macro_attribute]
pub fn worker(
    args: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let ast = parse(&args.into(), item.into());
    let model = analyze(ast);
    let ir = lower(&model);
    let rust = codegen(ir);
    rust.into()
}
