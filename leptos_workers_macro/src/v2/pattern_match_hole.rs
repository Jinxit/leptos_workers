use proc_macro2::{Delimiter, TokenStream, TokenTree};
use proc_macro_error::abort;
use quote::{quote_spanned, ToTokens};
use syn::spanned::Spanned;

pub fn pattern_match_holes(
    patterns: &[TokenStream],
    value: &(impl ToTokens + Clone),
) -> Option<TokenStream> {
    patterns
        .into_iter()
        .find_map(|p| pattern_match_hole(p, value))
}

pub fn pattern_match_hole(pattern: &TokenStream, value: &impl ToTokens) -> Option<TokenStream> {
    let hole_index = {
        let hole_indices = tokenize(pattern)
            .into_iter()
            .enumerate()
            .filter(|(_, tt)| tt.to_string() == "@")
            .collect::<Vec<_>>();
        if hole_indices.len() != 1 {
            abort!(
                value,
                format!(
                    "only 1 hole is supported \npattern: {:?}\n value: {:?}",
                    tokenize(pattern)
                        .into_iter()
                        .map(|tt| tt.to_string())
                        .collect::<Vec<_>>(),
                    tokenize(&value.into_token_stream())
                        .into_iter()
                        .map(|tt| tt.to_string())
                        .collect::<Vec<_>>()
                )
            )
        }
        hole_indices.first().unwrap().0
    };

    let part1 = tokenize(pattern)
        .into_iter()
        .take(hole_index)
        .into_iter()
        .collect::<Vec<_>>();
    let part2 = tokenize(pattern)
        .into_iter()
        .skip(hole_index + 1)
        .collect::<Vec<_>>();
    let token_stream = tokenize(&value.to_token_stream())
        .into_iter()
        .collect::<Vec<_>>();

    let part1_matches = token_stream
        .iter()
        .zip(part1.iter())
        .all(|(lhs, rhs)| lhs.to_string() == rhs.to_string());
    let part2_matches = token_stream
        .iter()
        .rev()
        .zip(part2.iter().rev())
        .all(|(lhs, rhs)| lhs.to_string() == rhs.to_string());

    if part1_matches && part2_matches {
        let start_index = part1.len();
        let stop_index = token_stream.len() - part2.len();
        let tokens = &token_stream[start_index..stop_index]
            .into_iter()
            .map(|s| str::parse::<TokenStream>(s).unwrap())
            .collect::<Vec<_>>();
        Some(quote_spanned!(value.span()=> #(#tokens)*))
    } else {
        None
    }
}

fn tokenize(token_stream: &TokenStream) -> Vec<String> {
    token_stream
        .clone()
        .into_iter()
        .flat_map(|tt| match tt {
            TokenTree::Group(group) => {
                let inner = group.stream().to_string();
                match group.delimiter() {
                    Delimiter::Parenthesis => vec!["(".to_string(), inner, ")".to_string()],
                    Delimiter::Brace => vec!["{".to_string(), inner, "}".to_string()],
                    Delimiter::Bracket => vec!["[".to_string(), inner, "]".to_string()],
                    Delimiter::None => vec!["".to_string(), inner, "".to_string()],
                }
            }
            TokenTree::Ident(ident) => vec![ident.to_string()],
            TokenTree::Punct(punct) => vec![punct.to_string()],
            TokenTree::Literal(lit) => vec![lit.to_string()],
        })
        .collect()
}
