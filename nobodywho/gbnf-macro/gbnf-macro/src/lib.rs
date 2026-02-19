use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Result, Token};

use gbnf_core::{
    is_at_new_declaration, parse_non_terminal, parse_token_ref, CharacterRange, Expr, Quantifier,
};

// ---------------------------------------------------------------------------
// MacroExpr: expression tree that supports interpolation
// ---------------------------------------------------------------------------

/// Macro-internal expression tree.
///
/// Mirrors `Expr` but adds `StringInterpolation` and `GrammarInclusion`
/// variants that hold `syn::Expr` (which doesn't implement `PartialEq`/`Eq`).
enum MacroExpr {
    /// A leaf GBNF expression (string literal, char range, nonterminal, token ref)
    Plain(Expr),
    /// A sequence of expressions
    Sequence(Vec<MacroExpr>),
    /// An alternation `a | b`
    Alternation(Vec<MacroExpr>),
    /// A grouped expression `(...)`
    Group(Box<MacroExpr>),
    /// A quantified expression
    Quantified {
        expr: Box<MacroExpr>,
        quantifier: Quantifier,
    },
    /// `{rust_expr}` — interpolate a runtime string as a terminal
    StringInterpolation(syn::Expr),
    /// `@{rust_expr}` — include a grammar
    GrammarInclusion(syn::Expr),
}

impl MacroExpr {
    /// Parse an atom: the smallest unit of a GBNF expression.
    fn parse_atom(input: ParseStream) -> Result<Self> {
        // @{expr} — grammar inclusion
        if input.peek(Token![@]) && input.peek2(syn::token::Brace) {
            input.parse::<Token![@]>()?;
            let content;
            syn::braced!(content in input);
            let grammar_expr: syn::Expr = content.parse()?;
            return Ok(MacroExpr::GrammarInclusion(grammar_expr));
        }

        // {expr} — string interpolation
        if input.peek(syn::token::Brace) {
            let content;
            syn::braced!(content in input);
            let string_expr: syn::Expr = content.parse()?;
            return Ok(MacroExpr::StringInterpolation(string_expr));
        }

        // Grouped expression (...)
        if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            let inner = MacroExpr::parse_alternation(&content)?;
            return Ok(MacroExpr::Group(Box::new(inner)));
        }

        // Token reference !<...> or <...>
        if (input.peek(Token![!]) && input.peek2(Token![<])) || input.peek(Token![<]) {
            let negated = if input.peek(Token![!]) {
                input.parse::<Token![!]>()?;
                true
            } else {
                false
            };
            let token_ref = parse_token_ref(input, negated)?;
            return Ok(MacroExpr::Plain(Expr::Token(token_ref)));
        }

        // String literal
        if input.peek(syn::LitStr) {
            let lit_str: syn::LitStr = input.parse()?;
            return Ok(MacroExpr::Plain(Expr::Characters(lit_str.value())));
        }

        // Character range [...]
        if input.peek(syn::token::Bracket) {
            let range: CharacterRange = input.parse()?;
            return Ok(MacroExpr::Plain(Expr::CharacterRange(range)));
        }

        // Non-terminal
        let name = parse_non_terminal(input)?;
        Ok(MacroExpr::Plain(Expr::NonTerminal(name)))
    }

    /// Parse a quantified atom: atom followed by optional quantifier.
    fn parse_quantified(input: ParseStream) -> Result<Self> {
        let atom = Self::parse_atom(input)?;

        if input.peek(Token![?]) || input.peek(Token![+]) || input.peek(Token![*]) {
            let quantifier: Quantifier = input.parse()?;
            Ok(MacroExpr::Quantified {
                expr: Box::new(atom),
                quantifier,
            })
        } else {
            Ok(atom)
        }
    }

    /// Parse a sequence: one or more quantified items.
    fn parse_sequence(input: ParseStream) -> Result<Self> {
        let mut items = vec![Self::parse_quantified(input)?];

        while !input.is_empty() && !input.peek(Token![|]) && !is_at_new_declaration(input) {
            items.push(Self::parse_quantified(input)?);
        }

        if items.len() == 1 {
            Ok(items.pop().unwrap())
        } else {
            Ok(MacroExpr::Sequence(items))
        }
    }

    /// Parse an alternation: sequences separated by `|`.
    fn parse_alternation(input: ParseStream) -> Result<Self> {
        let mut alternatives = vec![Self::parse_sequence(input)?];

        while input.peek(Token![|]) {
            input.parse::<Token![|]>()?;
            alternatives.push(Self::parse_sequence(input)?);
        }

        if alternatives.len() == 1 {
            Ok(alternatives.pop().unwrap())
        } else {
            Ok(MacroExpr::Alternation(alternatives))
        }
    }

    /// Generate tokens for constructing the runtime `Expr`.
    ///
    /// `StringInterpolation` becomes `Expr::Characters(expr.to_string())`.
    /// `GrammarInclusion` should have been extracted before this is called.
    fn to_tokens(&self) -> TokenStream2 {
        match self {
            MacroExpr::Plain(expr) => expr.to_tokens(),
            MacroExpr::Sequence(items) => {
                let item_tokens: Vec<_> = items.iter().map(|e| e.to_tokens()).collect();
                quote! { ::gbnf_core::Expr::Sequence(vec![#(#item_tokens),*]) }
            }
            MacroExpr::Alternation(alts) => {
                let alt_tokens: Vec<_> = alts.iter().map(|e| e.to_tokens()).collect();
                quote! { ::gbnf_core::Expr::Alternation(vec![#(#alt_tokens),*]) }
            }
            MacroExpr::Group(inner) => {
                let inner_tokens = inner.to_tokens();
                quote! { ::gbnf_core::Expr::Group(Box::new(#inner_tokens)) }
            }
            MacroExpr::Quantified { expr, quantifier } => {
                let expr_tokens = expr.to_tokens();
                let quant_tokens = quantifier.to_tokens();
                quote! {
                    ::gbnf_core::Expr::Quantified {
                        expr: Box::new(#expr_tokens),
                        quantifier: #quant_tokens,
                    }
                }
            }
            MacroExpr::StringInterpolation(e) => {
                quote! { ::gbnf_core::Expr::Characters((#e).to_string()) }
            }
            MacroExpr::GrammarInclusion(_) => {
                panic!("GrammarInclusion should have been extracted before codegen")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// MacroDeclaration / MacroInput
// ---------------------------------------------------------------------------

struct MacroDeclaration {
    name: String,
    expr: MacroExpr,
}

struct MacroInput {
    declarations: Vec<MacroDeclaration>,
}

impl Parse for MacroInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut declarations = Vec::new();

        while !input.is_empty() {
            let lhs = parse_non_terminal(input)?;
            input.parse::<Token![:]>()?;
            input.parse::<Token![:]>()?;
            input.parse::<Token![=]>()?;

            let rhs = MacroExpr::parse_alternation(input)?;
            declarations.push(MacroDeclaration {
                name: lhs,
                expr: rhs,
            });
        }

        Ok(MacroInput { declarations })
    }
}

// ---------------------------------------------------------------------------
// Grammar inclusion extraction
// ---------------------------------------------------------------------------

struct Inclusion {
    alias: String,
    grammar_expr: syn::Expr,
}

/// Walk all declarations and replace every `GrammarInclusion` node with a
/// `NonTerminal` reference to an auto-generated alias.
fn extract_grammar_inclusions(decls: &mut [MacroDeclaration]) -> Vec<Inclusion> {
    let mut inclusions = Vec::new();
    let mut counter: usize = 0;

    for decl in decls.iter_mut() {
        extract_from_expr(&mut decl.expr, &mut inclusions, &mut counter);
    }

    inclusions
}

fn extract_from_expr(
    expr: &mut MacroExpr,
    inclusions: &mut Vec<Inclusion>,
    counter: &mut usize,
) {
    match expr {
        MacroExpr::GrammarInclusion(grammar_expr) => {
            let alias = format!("_included-{}", *counter);
            *counter += 1;
            let grammar_expr = grammar_expr.clone();
            inclusions.push(Inclusion {
                alias: alias.clone(),
                grammar_expr,
            });
            *expr = MacroExpr::Plain(Expr::NonTerminal(alias));
        }
        MacroExpr::Sequence(items) => {
            for item in items.iter_mut() {
                extract_from_expr(item, inclusions, counter);
            }
        }
        MacroExpr::Alternation(alts) => {
            for alt in alts.iter_mut() {
                extract_from_expr(alt, inclusions, counter);
            }
        }
        MacroExpr::Group(inner) => {
            extract_from_expr(inner.as_mut(), inclusions, counter);
        }
        MacroExpr::Quantified { expr: inner, .. } => {
            extract_from_expr(inner.as_mut(), inclusions, counter);
        }
        MacroExpr::Plain(_) | MacroExpr::StringInterpolation(_) => {}
    }
}

// ---------------------------------------------------------------------------
// Proc macro entry point
// ---------------------------------------------------------------------------

#[proc_macro]
pub fn gbnf(input: TokenStream) -> TokenStream {
    let parsed = syn::parse_macro_input!(input as MacroInput);
    let mut declarations = parsed.declarations;
    let inclusions = extract_grammar_inclusions(&mut declarations);

    let mut builder_calls = quote! { ::gbnf_core::builder::GrammarBuilder::new() };

    for inc in &inclusions {
        let alias = &inc.alias;
        let grammar_expr = &inc.grammar_expr;
        builder_calls = quote! {
            #builder_calls.include_grammar_as(&#grammar_expr, #alias)
        };
    }

    for decl in &declarations {
        let name = &decl.name;
        let expr_tokens = decl.expr.to_tokens();
        builder_calls = quote! {
            #builder_calls.rule(#name, #expr_tokens)
        };
    }

    let expanded = quote! {
        { #builder_calls.build() }
    };

    TokenStream::from(expanded)
}
