//! GBNF Grammar types, builder, and composition utilities.
//!
//! This crate contains the core data types and runtime logic for GBNF grammars.
//! All compile-time concerns (parsing, code generation) live in `gbnf-macro`.

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
    /// Create a new GBNF grammar with an explicit root rule name.
    pub fn new(declarations: Vec<GbnfDeclaration>, root_name: String) -> Self {
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
}

// Helper functions for GBNF string generation

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
