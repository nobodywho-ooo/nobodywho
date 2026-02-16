use proc_macro::TokenStream;
use quote::quote;

#[proc_macro]
pub fn gbnf(input: TokenStream) -> TokenStream {
    let parsed = syn::parse_macro_input!(input as gbnf_types::GbnfInput);

    let decl_tokens: Vec<_> = parsed.declarations.iter().map(|d| d.to_tokens()).collect();

    let expanded = quote! {
        {
            let declarations = vec![#(#decl_tokens),*];
            ::gbnf_types::GbnfGrammar::new(declarations)
        }
    };

    TokenStream::from(expanded)
}
