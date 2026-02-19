//! GBNF Grammar types, builder, and composition utilities.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, Result, Token};

// GBNF docs reference:
// https://github.com/ggml-org/llama.cpp/blob/e9fd8dcab45d6cd147874e32565923bdfd0efbdb/grammars/README.md

pub mod builder;
pub mod compose;

/// A complete GBNF grammar containing multiple declarations
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GbnfGrammar {
    /// The AST declarations
    pub declarations: Vec<GbnfDeclaration>,
    /// The GBNF string representation
    pub gbnf_string: String,
    /// The name of the root rule (start symbol)
    pub root_name: String,
}

impl GbnfGrammar {
    /// Create a new GBNF grammar from declarations.
    /// The root rule is the first declaration's name.
    pub fn new(declarations: Vec<GbnfDeclaration>) -> Self {
        let root_name = declarations
            .first()
            .map(|d| d.name.clone())
            .unwrap_or_default();
        Self::new_with_root(declarations, root_name)
    }

    /// Create a new GBNF grammar with an explicit root rule name.
    pub fn new_with_root(declarations: Vec<GbnfDeclaration>, root_name: String) -> Self {
        let gbnf_string = declarations
            .iter()
            .map(|d| d.to_gbnf())
            .collect::<Vec<_>>()
            .join("\n");
        Self {
            declarations,
            gbnf_string,
            root_name,
        }
    }

    /// Get the GBNF string representation
    pub fn as_str(&self) -> &str {
        &self.gbnf_string
    }
}

/// A single GBNF rule declaration (e.g., `root ::= "hello"`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GbnfDeclaration {
    /// The left-hand side (rule name)
    pub name: String,
    /// The right-hand side expression
    pub expr: Expr,
}

impl GbnfDeclaration {
    /// Create a new declaration
    pub fn new(name: String, expr: Expr) -> Self {
        Self { name, expr }
    }

    /// Convert to GBNF string representation
    pub fn to_gbnf(&self) -> String {
        format!("{} ::= {}", self.name, self.expr.to_gbnf())
    }

    /// Generate tokens for constructing this declaration at compile time
    pub fn to_tokens(&self) -> TokenStream2 {
        let name = &self.name;
        let expr_tokens = self.expr.to_tokens();
        quote! {
            ::gbnf_core::GbnfDeclaration::new(#name.to_string(), #expr_tokens)
        }
    }
}

impl Parse for GbnfDeclaration {
    fn parse(input: ParseStream) -> Result<Self> {
        // Parse and validate non-terminal identifier
        let lhs = parse_non_terminal(input)?;

        // `::=` assignment
        input.parse::<Token![:]>()?;
        input.parse::<Token![:]>()?;
        input.parse::<Token![=]>()?;

        // Parse the right-hand side expression
        let rhs: Expr = input.parse()?;

        Ok(GbnfDeclaration {
            name: lhs,
            expr: rhs,
        })
    }
}

/// A character range specification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CharacterRange {
    /// A range like [a-z]
    Range {
        begin: char,
        end: char,
        negated: bool,
    },
    /// A character set like [abc]
    Set { chars: Vec<char>, negated: bool },
}

impl CharacterRange {
    /// Convert to GBNF string representation
    pub fn to_gbnf(&self) -> String {
        match self {
            CharacterRange::Range {
                begin,
                end,
                negated,
            } => {
                let neg = if *negated { "^" } else { "" };
                format!("[{}{}-{}]", neg, escape_char(*begin), escape_char(*end))
            }
            CharacterRange::Set { chars, negated } => {
                let neg = if *negated { "^" } else { "" };
                // Put '-' at the end so it's not interpreted as a range delimiter
                let has_dash = chars.contains(&'-');
                let chars_str: String = chars
                    .iter()
                    .filter(|&&c| c != '-')
                    .map(|c| escape_char(*c))
                    .collect();
                let dash = if has_dash { "-" } else { "" };
                format!("[{}{}{}]", neg, chars_str, dash)
            }
        }
    }

    /// Generate tokens for constructing this range at compile time
    pub fn to_tokens(&self) -> TokenStream2 {
        match self {
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
}

impl Parse for CharacterRange {
    fn parse(input: ParseStream) -> Result<Self> {
        use syn::bracketed;

        let content;
        bracketed!(content in input);

        // Check for negation
        let negated = if content.peek(Token![^]) {
            content.parse::<Token![^]>()?;
            true
        } else {
            false
        };

        // Parse the first character
        let first_char = parse_char(&content)?;

        // Check if this is a range (char-char) or a set
        if content.peek(Token![-]) {
            // This is a range: [a-z] or [^a-z]
            content.parse::<Token![-]>()?;
            let end_char = parse_char(&content)?;

            Ok(CharacterRange::Range {
                begin: first_char,
                end: end_char,
                negated,
            })
        } else {
            // This is a character set: [abc] or [^abc]
            let mut chars = vec![first_char];

            while !content.is_empty() {
                chars.push(parse_char(&content)?);
            }

            Ok(CharacterRange::Set { chars, negated })
        }
    }
}

/// A quantifier for repetition
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Quantifier {
    /// `?` - zero or one
    Optional,
    /// `+` - one or more
    OneOrMore,
    /// `*` - zero or more
    ZeroOrMore,
    /// `{n}` - exactly n
    Exact(usize),
    /// `{n,}` - at least n
    AtLeast(usize),
    /// `{n,m}` - between n and m
    Range(usize, usize),
}

impl Quantifier {
    /// Convert to GBNF string representation
    pub fn to_gbnf(&self) -> String {
        match self {
            Quantifier::Optional => "?".to_string(),
            Quantifier::OneOrMore => "+".to_string(),
            Quantifier::ZeroOrMore => "*".to_string(),
            Quantifier::Exact(n) => format!("{{{}}}", n),
            Quantifier::AtLeast(n) => format!("{{{},}}", n),
            Quantifier::Range(n, m) => format!("{{{},{}}}", n, m),
        }
    }

    /// Generate tokens for constructing this quantifier at compile time
    pub fn to_tokens(&self) -> TokenStream2 {
        match self {
            Quantifier::Optional => quote! { ::gbnf_core::Quantifier::Optional },
            Quantifier::OneOrMore => quote! { ::gbnf_core::Quantifier::OneOrMore },
            Quantifier::ZeroOrMore => quote! { ::gbnf_core::Quantifier::ZeroOrMore },
            Quantifier::Exact(n) => quote! { ::gbnf_core::Quantifier::Exact(#n) },
            Quantifier::AtLeast(n) => quote! { ::gbnf_core::Quantifier::AtLeast(#n) },
            Quantifier::Range(n, m) => quote! { ::gbnf_core::Quantifier::Range(#n, #m) },
        }
    }
}

impl Parse for Quantifier {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Token![?]) {
            input.parse::<Token![?]>()?;
            Ok(Quantifier::Optional)
        } else if input.peek(Token![+]) {
            input.parse::<Token![+]>()?;
            Ok(Quantifier::OneOrMore)
        } else if input.peek(Token![*]) {
            input.parse::<Token![*]>()?;
            Ok(Quantifier::ZeroOrMore)
        } else if input.peek(syn::token::Brace) {
            use syn::braced;
            let content;
            braced!(content in input);

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
        } else {
            Err(input.error("expected quantifier: ?, +, *, or {n}, {n,}, {n,m}"))
        }
    }
}

/// A token reference
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenRef {
    /// `<[1000]>` - by ID
    ById { id: usize, negated: bool },
    /// `<think>` - by string
    ByString { name: String, negated: bool },
}

impl TokenRef {
    /// Convert to GBNF string representation
    pub fn to_gbnf(&self) -> String {
        match self {
            TokenRef::ById { id, negated } => {
                let neg = if *negated { "!" } else { "" };
                format!("{}<[{}]>", neg, id)
            }
            TokenRef::ByString { name, negated } => {
                let neg = if *negated { "!" } else { "" };
                format!("{}<{}>", neg, name)
            }
        }
    }

    /// Generate tokens for constructing this token ref at compile time
    pub fn to_tokens(&self) -> TokenStream2 {
        match self {
            TokenRef::ById { id, negated } => {
                quote! {
                    ::gbnf_core::TokenRef::ById { id: #id, negated: #negated }
                }
            }
            TokenRef::ByString { name, negated } => {
                quote! {
                    ::gbnf_core::TokenRef::ByString { name: #name.to_string(), negated: #negated }
                }
            }
        }
    }
}

/// An expression in a GBNF rule
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// A string literal like `"hello"`
    Characters(String),
    /// A character range like `[a-z]`
    CharacterRange(CharacterRange),
    /// A token reference like `<think>`
    Token(TokenRef),
    /// A reference to another rule
    NonTerminal(String),
    /// A grouped expression `(...)`
    Group(Box<Expr>),
    /// A sequence of expressions
    Sequence(Vec<Expr>),
    /// An alternation `a | b`
    Alternation(Vec<Expr>),
    /// A quantified expression
    Quantified {
        expr: Box<Expr>,
        quantifier: Quantifier,
    },
}

impl Expr {
    /// Convert to GBNF string representation
    pub fn to_gbnf(&self) -> String {
        match self {
            Expr::Characters(s) => format!("\"{}\"", escape_string(s)),
            Expr::CharacterRange(r) => r.to_gbnf(),
            Expr::Token(t) => t.to_gbnf(),
            Expr::NonTerminal(name) => name.clone(),
            Expr::Group(inner) => format!("({})", inner.to_gbnf()),
            Expr::Sequence(items) => items
                .iter()
                .map(|e| e.to_gbnf())
                .collect::<Vec<_>>()
                .join(" "),
            Expr::Alternation(alts) => alts
                .iter()
                .map(|e| e.to_gbnf())
                .collect::<Vec<_>>()
                .join(" | "),
            Expr::Quantified { expr, quantifier } => {
                let expr_str = expr.to_gbnf();
                // Add parens if needed for precedence
                let expr_str = match expr.as_ref() {
                    Expr::Alternation(_) | Expr::Sequence(_) => format!("({})", expr_str),
                    _ => expr_str,
                };
                format!("{}{}", expr_str, quantifier.to_gbnf())
            }
        }
    }

    /// Generate tokens for constructing this expression at compile time
    pub fn to_tokens(&self) -> TokenStream2 {
        match self {
            Expr::Characters(s) => {
                quote! { ::gbnf_core::Expr::Characters(#s.to_string()) }
            }
            Expr::CharacterRange(r) => {
                let range_tokens = r.to_tokens();
                quote! { ::gbnf_core::Expr::CharacterRange(#range_tokens) }
            }
            Expr::Token(t) => {
                let token_tokens = t.to_tokens();
                quote! { ::gbnf_core::Expr::Token(#token_tokens) }
            }
            Expr::NonTerminal(name) => {
                quote! { ::gbnf_core::Expr::NonTerminal(#name.to_string()) }
            }
            Expr::Group(inner) => {
                let inner_tokens = inner.to_tokens();
                quote! { ::gbnf_core::Expr::Group(Box::new(#inner_tokens)) }
            }
            Expr::Sequence(items) => {
                let item_tokens: Vec<_> = items.iter().map(|e| e.to_tokens()).collect();
                quote! { ::gbnf_core::Expr::Sequence(vec![#(#item_tokens),*]) }
            }
            Expr::Alternation(alts) => {
                let alt_tokens: Vec<_> = alts.iter().map(|e| e.to_tokens()).collect();
                quote! { ::gbnf_core::Expr::Alternation(vec![#(#alt_tokens),*]) }
            }
            Expr::Quantified { expr, quantifier } => {
                let expr_tokens = expr.to_tokens();
                let quant_tokens = quantifier.to_tokens();
                quote! {
                    ::gbnf_core::Expr::Quantified {
                        expr: Box::new(#expr_tokens),
                        quantifier: #quant_tokens,
                    }
                }
            }
        }
    }

    // Parse an atom: terminal, non-terminal, group, or token
    fn parse_atom(input: ParseStream) -> Result<Self> {
        // Check for grouped expression (...)
        if input.peek(syn::token::Paren) {
            use syn::parenthesized;
            let content;
            parenthesized!(content in input);
            let inner: Expr = content.parse()?;
            return Ok(Expr::Group(Box::new(inner)));
        }

        // Check for token !<...> or <...>
        if (input.peek(Token![!]) && input.peek2(Token![<])) || input.peek(Token![<]) {
            let negated = if input.peek(Token![!]) {
                input.parse::<Token![!]>()?;
                true
            } else {
                false
            };
            let token_ref = parse_token_ref(input, negated)?;
            return Ok(Expr::Token(token_ref));
        }

        // Try to parse as string literal
        if input.peek(syn::LitStr) {
            let lit_str: syn::LitStr = input.parse()?;
            return Ok(Expr::Characters(lit_str.value()));
        }

        // Try to parse as character range
        if input.peek(syn::token::Bracket) {
            let range: CharacterRange = input.parse()?;
            return Ok(Expr::CharacterRange(range));
        }

        // Otherwise, parse as non-terminal
        let name = parse_non_terminal(input)?;
        Ok(Expr::NonTerminal(name))
    }

    // Parse a quantified atom: atom followed by optional quantifier
    fn parse_quantified(input: ParseStream) -> Result<Self> {
        let atom = Self::parse_atom(input)?;

        // Check for quantifier
        if input.peek(Token![?])
            || input.peek(Token![+])
            || input.peek(Token![*])
            || input.peek(syn::token::Brace)
        {
            let quantifier: Quantifier = input.parse()?;
            Ok(Expr::Quantified {
                expr: Box::new(atom),
                quantifier,
            })
        } else {
            Ok(atom)
        }
    }

    // Parse a sequence: one or more quantified atoms
    fn parse_sequence(input: ParseStream) -> Result<Self> {
        let mut items = vec![Self::parse_quantified(input)?];

        // Keep parsing while we see more atoms (not | or end)
        while !input.is_empty() && !input.peek(Token![|]) && !is_at_new_declaration(input) {
            items.push(Self::parse_quantified(input)?);
        }

        if items.len() == 1 {
            Ok(items.pop().unwrap())
        } else {
            Ok(Expr::Sequence(items))
        }
    }
}

impl Parse for Expr {
    // Parse alternation: sequences separated by |
    fn parse(input: ParseStream) -> Result<Self> {
        let mut alternatives = vec![Expr::parse_sequence(input)?];

        while input.peek(Token![|]) {
            input.parse::<Token![|]>()?;
            alternatives.push(Expr::parse_sequence(input)?);
        }

        if alternatives.len() == 1 {
            Ok(alternatives.pop().unwrap())
        } else {
            Ok(Expr::Alternation(alternatives))
        }
    }
}

// Helper functions for parsing

/// Parse and validate a non-terminal symbol (must be lowercase with dashes)
/// Handles hyphenated names like `json-string` by consuming ident-dash-ident sequences
pub fn parse_non_terminal(input: ParseStream) -> Result<String> {
    let first_ident: Ident = input.parse()?;
    let first_name = first_ident.to_string();

    // Validate first part: must be lowercase letters only
    if !first_name.chars().all(|c| c.is_lowercase()) {
        return Err(syn::Error::new_spanned(
            first_ident,
            "non-terminal symbols must be lowercase words (e.g., 'move', 'castle', 'json-string')",
        ));
    }

    let mut name = first_name;

    // Keep consuming -ident sequences
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

/// Parse a single character from various token forms
pub fn parse_char(input: ParseStream) -> Result<char> {
    // Try to parse as a character literal (like '\t', ' ', 'a')
    if input.peek(syn::LitChar) {
        let lit: syn::LitChar = input.parse()?;
        return Ok(lit.value());
    }

    // Try to parse as an identifier (like a, z, etc.)
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

    // Try to parse as a literal (for numbers like 0, 9)
    if let Ok(lit) = input.parse::<syn::LitInt>() {
        let s = lit.to_string();
        if s.len() == 1 {
            return Ok(s.chars().next().unwrap());
        }
        return Err(syn::Error::new_spanned(lit, "expected a single digit"));
    }

    Err(input.error("expected a character (e.g., a, 0, or '\\t')"))
}

/// Parse token references: <[1000]> or <think>
pub fn parse_token_ref(input: ParseStream, negated: bool) -> Result<TokenRef> {
    input.parse::<Token![<]>()?;

    let token_ref = if input.peek(syn::token::Bracket) {
        // <[1000]> - by ID
        use syn::bracketed;
        let content;
        bracketed!(content in input);
        let id: syn::LitInt = content.parse()?;
        TokenRef::ById {
            id: id.base10_parse()?,
            negated,
        }
    } else {
        // <think> - by string
        let ident: Ident = input.parse()?;
        TokenRef::ByString {
            name: ident.to_string(),
            negated,
        }
    };

    input.parse::<Token![>]>()?;
    Ok(token_ref)
}

/// Check if we're at the start of a new declaration
/// Handles hyphenated names like `json-string ::= ...`
pub fn is_at_new_declaration(input: ParseStream) -> bool {
    let fork = input.fork();

    // Parse the first identifier
    if fork.parse::<Ident>().is_err() {
        return false;
    }

    // Skip any -ident sequences (for hyphenated names)
    while fork.peek(Token![-]) && fork.peek2(Ident) {
        if fork.parse::<Token![-]>().is_err() {
            return false;
        }
        if fork.parse::<Ident>().is_err() {
            return false;
        }
    }

    // Now check for ::=
    fork.parse::<Token![:]>().is_ok()
        && fork.parse::<Token![:]>().is_ok()
        && fork.parse::<Token![=]>().is_ok()
}

// Helper functions for string generation

/// Escape a character for GBNF character ranges
/// Note: Inside [...] character classes, " does NOT need escaping in llama.cpp GBNF
fn escape_char(c: char) -> String {
    match c {
        '\n' => "\\n".to_string(),
        '\r' => "\\r".to_string(),
        '\t' => "\\t".to_string(),
        '\\' => "\\\\".to_string(),
        ']' => "\\]".to_string(),
        '^' => "\\^".to_string(),
        // Control characters (0x00-0x1F) and DEL (0x7F) as hex escapes
        c if c.is_ascii_control() => format!("\\x{:02X}", c as u32),
        // Note: '-' is handled specially in CharacterRange::Set::to_gbnf()
        // by placing it at the end of the set
        _ => c.to_string(),
    }
}

/// Escape a string for GBNF string literals
fn escape_string(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\t' => "\\t".to_string(),
            '\\' => "\\\\".to_string(),
            '"' => "\\\"".to_string(),
            _ => c.to_string(),
        })
        .collect()
}
