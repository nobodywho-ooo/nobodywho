use proc_macro::TokenStream;
use quote::quote;

#[proc_macro]
pub fn gbnf(input: TokenStream) -> TokenStream {
    let parsed = syn::parse_macro_input!(input as gbnf_types::GbnfInput);

    let expanded = if parsed.has_inclusions() {
        // Composition path: generate GrammarBuilder chain
        let mut builder_calls = quote! { ::gbnf::builder::GrammarBuilder::new() };

        for stmt in &parsed.statements {
            match stmt {
                gbnf_types::GbnfStatement::Inclusion { name, grammar_expr } => {
                    builder_calls = quote! {
                        #builder_calls.include_grammar_as(&#grammar_expr, #name)
                    };
                }
                gbnf_types::GbnfStatement::Declaration(decl) => {
                    let name = &decl.name;
                    let expr_tokens = decl.expr.to_tokens();
                    builder_calls = quote! {
                        #builder_calls.rule(#name, #expr_tokens)
                    };
                }
            }
        }

        quote! {
            { #builder_calls.build() }
        }
    } else {
        // Simple path: direct vec construction (no builder overhead)
        let decl_tokens: Vec<_> = parsed
            .statements
            .iter()
            .map(|s| match s {
                gbnf_types::GbnfStatement::Declaration(d) => d.to_tokens(),
                _ => unreachable!(),
            })
            .collect();

        quote! {
            {
                let declarations = vec![#(#decl_tokens),*];
                ::gbnf_types::GbnfGrammar::new(declarations)
            }
        }
    };

    TokenStream::from(expanded)
}
