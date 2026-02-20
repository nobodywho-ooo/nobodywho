use std::collections::HashSet;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, Result, Token};

use gbnf_core::{CharacterRange, Expr, Quantifier, TokenRef};

// ---------------------------------------------------------------------------
// Parsing helpers (moved from gbnf-core)
// ---------------------------------------------------------------------------

/// Parse and validate a non-terminal symbol (must be lowercase with dashes).
/// Handles hyphenated names like `json-string` by consuming ident-dash-ident sequences.
fn parse_non_terminal(input: ParseStream) -> Result<String> {
    let first_ident: Ident = input.parse()?;
    let first_name = first_ident.to_string();

    if !first_name.chars().all(|c| c.is_lowercase()) {
        return Err(syn::Error::new_spanned(
            first_ident,
            "non-terminal symbols must be lowercase words (e.g., 'move', 'castle', 'json-string')",
        ));
    }

    let mut name = first_name;

    while input.peek(Token![-]) && input.peek2(Ident) {
        input.parse::<Token![-]>()?;
        let next_ident: Ident = input.parse()?;
        let next_name = next_ident.to_string();

        if !next_name.chars().all(|c| c.is_lowercase()) {
            return Err(syn::Error::new_spanned(
                next_ident,
                "non-terminal symbols must be lowercase words",
            ));
        }

        name.push('-');
        name.push_str(&next_name);
    }

    Ok(name)
}

/// Parse a single character from various token forms.
fn parse_char(input: ParseStream) -> Result<char> {
    if input.peek(syn::LitChar) {
        let lit: syn::LitChar = input.parse()?;
        return Ok(lit.value());
    }

    if let Ok(ident) = input.parse::<Ident>() {
        let s = ident.to_string();
        if s.len() == 1 {
            return Ok(s.chars().next().unwrap());
        }
        return Err(syn::Error::new_spanned(
            ident,
            "expected a single character",
        ));
    }

    if let Ok(lit) = input.parse::<syn::LitInt>() {
        let s = lit.to_string();
        if s.len() == 1 {
            return Ok(s.chars().next().unwrap());
        }
        return Err(syn::Error::new_spanned(lit, "expected a single digit"));
    }

    Err(input.error("expected a character (e.g., a, 0, or '\\t')"))
}

/// Parse a token reference: `<[1000]>` or `<think>`.
fn parse_token_ref(input: ParseStream, negated: bool) -> Result<TokenRef> {
    input.parse::<Token![<]>()?;

    let token_ref = if input.peek(syn::token::Bracket) {
        use syn::bracketed;
        let content;
        bracketed!(content in input);
        let id: syn::LitInt = content.parse()?;
        TokenRef::ById {
            id: id.base10_parse()?,
            negated,
        }
    } else {
        let ident: Ident = input.parse()?;
        TokenRef::ByString {
            name: ident.to_string(),
            negated,
        }
    };

    input.parse::<Token![>]>()?;
    Ok(token_ref)
}

/// Parse a character range: `[a-z]`, `[^a-z]`, `[abc]`, etc.
fn parse_character_range(input: ParseStream) -> Result<CharacterRange> {
    use syn::bracketed;

    let content;
    bracketed!(content in input);

    let negated = if content.peek(Token![^]) {
        content.parse::<Token![^]>()?;
        true
    } else {
        false
    };

    let first_char = parse_char(&content)?;

    if content.peek(Token![-]) {
        content.parse::<Token![-]>()?;
        let end_char = parse_char(&content)?;

        Ok(CharacterRange::Range {
            begin: first_char,
            end: end_char,
            negated,
        })
    } else {
        let mut chars = vec![first_char];

        while !content.is_empty() {
            chars.push(parse_char(&content)?);
        }

        Ok(CharacterRange::Set { chars, negated })
    }
}

/// Parse a quantifier: `?`, `+`, `*`.
fn parse_quantifier(input: ParseStream) -> Result<Quantifier> {
    if input.peek(Token![?]) {
        input.parse::<Token![?]>()?;
        Ok(Quantifier::Optional)
    } else if input.peek(Token![+]) {
        input.parse::<Token![+]>()?;
        Ok(Quantifier::OneOrMore)
    } else if input.peek(Token![*]) {
        input.parse::<Token![*]>()?;
        Ok(Quantifier::ZeroOrMore)
    } else {
        Err(input.error("expected quantifier: ?, +, or *"))
    }
}

/// Parse a brace quantifier from already-opened brace content: `{n}`, `{n,}`, `{n,m}`.
fn parse_brace_quantifier(content: ParseStream) -> Result<Quantifier> {
    let first: syn::LitInt = content.parse()?;
    let first_val: usize = first.base10_parse()?;

    if content.is_empty() {
        Ok(Quantifier::Exact(first_val))
    } else {
        content.parse::<Token![,]>()?;
        if content.is_empty() {
            Ok(Quantifier::AtLeast(first_val))
        } else {
            let second: syn::LitInt = content.parse()?;
            let second_val: usize = second.base10_parse()?;
            Ok(Quantifier::Range(first_val, second_val))
        }
    }
}

/// Check if we're at the start of a new declaration.
/// Handles hyphenated names like `json-string ::= ...`.
fn is_at_new_declaration(input: ParseStream) -> bool {
    let fork = input.fork();

    if fork.parse::<Ident>().is_err() {
        return false;
    }

    while fork.peek(Token![-]) && fork.peek2(Ident) {
        if fork.parse::<Token![-]>().is_err() {
            return false;
        }
        if fork.parse::<Ident>().is_err() {
            return false;
        }
    }

    fork.parse::<Token![:]>().is_ok()
        && fork.parse::<Token![:]>().is_ok()
        && fork.parse::<Token![=]>().is_ok()
}

// ---------------------------------------------------------------------------
// Code generation: convert AST types to token streams
// ---------------------------------------------------------------------------

fn expr_to_tokens(expr: &Expr) -> TokenStream2 {
    match expr {
        Expr::Characters(s) => {
            quote! { ::gbnf_core::Expr::Characters(#s.to_string()) }
        }
        Expr::CharacterRange(r) => {
            let range_tokens = character_range_to_tokens(r);
            quote! { ::gbnf_core::Expr::CharacterRange(#range_tokens) }
        }
        Expr::Token(t) => {
            let token_tokens = token_ref_to_tokens(t);
            quote! { ::gbnf_core::Expr::Token(#token_tokens) }
        }
        Expr::NonTerminal(name) => {
            quote! { ::gbnf_core::Expr::NonTerminal(#name.to_string()) }
        }
        Expr::Group(inner) => {
            let inner_tokens = expr_to_tokens(inner);
            quote! { ::gbnf_core::Expr::Group(Box::new(#inner_tokens)) }
        }
        Expr::Sequence(items) => {
            let item_tokens: Vec<_> = items.iter().map(|e| expr_to_tokens(e)).collect();
            quote! { ::gbnf_core::Expr::Sequence(vec![#(#item_tokens),*]) }
        }
        Expr::Alternation(alts) => {
            let alt_tokens: Vec<_> = alts.iter().map(|e| expr_to_tokens(e)).collect();
            quote! { ::gbnf_core::Expr::Alternation(vec![#(#alt_tokens),*]) }
        }
        Expr::Quantified { expr, quantifier } => {
            let expr_tokens = expr_to_tokens(expr);
            let quant_tokens = quantifier_to_tokens(quantifier);
            quote! {
                ::gbnf_core::Expr::Quantified {
                    expr: Box::new(#expr_tokens),
                    quantifier: #quant_tokens,
                }
            }
        }
    }
}

fn character_range_to_tokens(range: &CharacterRange) -> TokenStream2 {
    match range {
        CharacterRange::Range {
            begin,
            end,
            negated,
        } => {
            quote! {
                ::gbnf_core::CharacterRange::Range {
                    begin: #begin,
                    end: #end,
                    negated: #negated,
                }
            }
        }
        CharacterRange::Set { chars, negated } => {
            quote! {
                ::gbnf_core::CharacterRange::Set {
                    chars: vec![#(#chars),*],
                    negated: #negated,
                }
            }
        }
    }
}

fn token_ref_to_tokens(token_ref: &TokenRef) -> TokenStream2 {
    match token_ref {
        TokenRef::ById { id, negated } => {
            quote! { ::gbnf_core::TokenRef::ById { id: #id, negated: #negated } }
        }
        TokenRef::ByString { name, negated } => {
            quote! { ::gbnf_core::TokenRef::ByString { name: #name.to_string(), negated: #negated } }
        }
    }
}

fn quantifier_to_tokens(quantifier: &Quantifier) -> TokenStream2 {
    match quantifier {
        Quantifier::Optional => quote! { ::gbnf_core::Quantifier::Optional },
        Quantifier::OneOrMore => quote! { ::gbnf_core::Quantifier::OneOrMore },
        Quantifier::ZeroOrMore => quote! { ::gbnf_core::Quantifier::ZeroOrMore },
        Quantifier::Exact(n) => quote! { ::gbnf_core::Quantifier::Exact(#n) },
        Quantifier::AtLeast(n) => quote! { ::gbnf_core::Quantifier::AtLeast(#n) },
        Quantifier::Range(n, m) => quote! { ::gbnf_core::Quantifier::Range(#n, #m) },
    }
}

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
            let range = parse_character_range(input)?;
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
            let quantifier = parse_quantifier(input)?;
            Ok(MacroExpr::Quantified {
                expr: Box::new(atom),
                quantifier,
            })
        } else if input.peek(syn::token::Brace) {
            // Disambiguate {n}/{n,}/{n,m} quantifier vs {expr} interpolation.
            // Peek inside: if it starts with an integer, it's a quantifier.
            let fork = input.fork();
            let inner;
            syn::braced!(inner in fork);
            if inner.peek(syn::LitInt) {
                // It's a quantifier — parse from the real input.
                let content;
                syn::braced!(content in input);
                let quantifier = parse_brace_quantifier(&content)?;
                Ok(MacroExpr::Quantified {
                    expr: Box::new(atom),
                    quantifier,
                })
            } else {
                // Not a quantifier — leave {expr} for the next parse_atom call.
                Ok(atom)
            }
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
    fn to_tokens(&self) -> TokenStream2 {
        match self {
            MacroExpr::Plain(expr) => expr_to_tokens(expr),
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
                let quant_tokens = quantifier_to_tokens(quantifier);
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

fn extract_from_expr(expr: &mut MacroExpr, inclusions: &mut Vec<Inclusion>, counter: &mut usize) {
    match expr {
        MacroExpr::GrammarInclusion(grammar_expr) => {
            let alias = format!("included-{}", *counter);
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
// Validation
// ---------------------------------------------------------------------------

/// Collect all nonterminal references from a MacroExpr tree.
fn collect_nonterminal_refs(expr: &MacroExpr, refs: &mut Vec<String>) {
    match expr {
        MacroExpr::Plain(Expr::NonTerminal(name)) => refs.push(name.clone()),
        MacroExpr::Plain(_) | MacroExpr::StringInterpolation(_) => {}
        MacroExpr::Sequence(items) => {
            for item in items {
                collect_nonterminal_refs(item, refs);
            }
        }
        MacroExpr::Alternation(alts) => {
            for alt in alts {
                collect_nonterminal_refs(alt, refs);
            }
        }
        MacroExpr::Group(inner) => collect_nonterminal_refs(inner, refs),
        MacroExpr::Quantified { expr, .. } => collect_nonterminal_refs(expr, refs),
        MacroExpr::GrammarInclusion(_) => {} // already extracted
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

    // Validate: all nonterminal references must resolve to a declared rule or inclusion alias.
    let known_names: HashSet<String> = declarations
        .iter()
        .map(|d| d.name.clone())
        .chain(inclusions.iter().map(|i| i.alias.clone()))
        .collect();

    let mut all_refs = Vec::new();
    for decl in &declarations {
        collect_nonterminal_refs(&decl.expr, &mut all_refs);
    }

    let undefined: Vec<&String> = all_refs
        .iter()
        .filter(|r| !known_names.contains(r.as_str()))
        .collect();

    if !undefined.is_empty() {
        let names: Vec<&str> = undefined.iter().map(|s| s.as_str()).collect();
        let msg = format!("undefined nonterminal(s): {}", names.join(", "));
        return syn::Error::new(proc_macro2::Span::call_site(), msg)
            .to_compile_error()
            .into();
    }

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

    // Root rule: use "root" if a declaration with that name exists, otherwise first declaration.
    let root_name = declarations
        .iter()
        .find(|d| d.name == "root")
        .or(declarations.first())
        .map(|d| d.name.as_str())
        .unwrap_or("root");

    let expanded = quote! {
        { #builder_calls.root(#root_name).build() }
    };

    TokenStream::from(expanded)
}
