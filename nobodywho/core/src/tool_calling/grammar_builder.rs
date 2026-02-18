//! Helper utilities for building GBNF grammars with less boilerplate.
//!
//! This module provides a builder pattern and convenience functions for constructing
//! GBNF grammars programmatically, making it easier to create custom grammar rules
//! for constrained LLM output generation.

use gbnf::{CharacterRange, Expr, GbnfDeclaration, GbnfGrammar, Quantifier};

/// Builder for constructing GBNF grammars.
///
/// Provides a fluent interface for adding rules to a grammar.
///
/// # Example
/// ```ignore
/// use crate::tool_calling::grammar_builder::{GrammarBuilder, t, nt, seq};
///
/// let grammar = GrammarBuilder::new()
///     .rule("greeting", seq(&[t("Hello"), t(" "), nt("name")]))
///     .rule("name", t("World"))
///     .build();
/// ```
pub struct GrammarBuilder {
    declarations: Vec<GbnfDeclaration>,
}

impl GrammarBuilder {
    /// Create a new grammar builder with an empty grammar.
    pub fn new() -> Self {
        Self {
            declarations: Vec::new(),
        }
    }

    /// Create a grammar builder from an existing grammar.
    ///
    /// This allows you to add more rules to an existing grammar, which is useful
    /// when you have a base grammar (e.g., from JSON schema) and want to extend it.
    ///
    /// # Example
    /// ```ignore
    /// let json_grammar = json_schema_to_grammar(&schema)?;
    /// let extended = GrammarBuilder::from_existing(json_grammar)
    ///     .rule("wrapper", seq(&[t("<start>"), nt("root"), t("<end>")]))
    ///     .build();
    /// ```
    pub fn from_existing(grammar: GbnfGrammar) -> Self {
        Self {
            declarations: grammar.declarations,
        }
    }

    /// Add a rule to the grammar.
    ///
    /// Rules define productions in the grammar and are processed in order.
    pub fn rule(mut self, name: &str, expr: Expr) -> Self {
        self.declarations
            .push(GbnfDeclaration::new(name.to_string(), expr));
        self
    }

    /// Build and return the final grammar.
    pub fn build(self) -> GbnfGrammar {
        GbnfGrammar::new(self.declarations)
    }
}

impl Default for GrammarBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a terminal expression (exact text match).
///
/// # Example
/// ```ignore
/// let expr = t("hello"); // Matches exactly "hello"
/// ```
pub fn t(s: &str) -> Expr {
    Expr::Characters(s.to_string())
}

/// Create a non-terminal reference.
///
/// This references another rule in the grammar (exactly once).
///
/// # Example
/// ```ignore
/// let expr = nt("my_rule"); // References the rule named "my_rule"
/// ```
pub fn nt(name: &str) -> Expr {
    Expr::NonTerminal(name.to_string())
}

/// Create a non-terminal reference with one-or-more repetition.
///
/// This references another rule that must occur at least once (like `+` in regex).
///
/// # Example
/// ```ignore
/// let expr = nt_plus("item"); // Matches one or more "item"
/// ```
pub fn nt_plus(name: &str) -> Expr {
    Expr::Quantified {
        expr: Box::new(Expr::NonTerminal(name.to_string())),
        quantifier: Quantifier::OneOrMore,
    }
}

/// Create a non-terminal reference with zero-or-more repetition.
///
/// This references another rule that can occur zero or more times (like `*` in regex).
///
/// # Example
/// ```ignore
/// let expr = nt_star("item"); // Matches zero or more "item"
/// ```
pub fn nt_star(name: &str) -> Expr {
    Expr::Quantified {
        expr: Box::new(Expr::NonTerminal(name.to_string())),
        quantifier: Quantifier::ZeroOrMore,
    }
}

/// Create a terminal with zero-or-more repetition.
///
/// This matches the exact string zero or more times (Kleene star).
///
/// # Example
/// ```ignore
/// let expr = t_star(" "); // Matches zero or more spaces
/// ```
pub fn t_star(s: &str) -> Expr {
    Expr::Quantified {
        expr: Box::new(Expr::Characters(s.to_string())),
        quantifier: Quantifier::ZeroOrMore,
    }
}

/// Create a sequence from multiple expressions.
///
/// # Example
/// ```ignore
/// let expr = seq(&[t("hello"), t(" "), nt("name")]);
/// ```
pub fn seq(items: &[Expr]) -> Expr {
    if items.len() == 1 {
        items[0].clone()
    } else {
        Expr::Sequence(items.to_vec())
    }
}

/// Create a character set that matches anything except the given characters.
///
/// Uses one-or-more repetition, so it must match at least one character.
///
/// # Example
/// ```ignore
/// let expr = not_chars(&['<', '>', '{', '}']); // Matches any char except < > { }
/// ```
pub fn not_chars(chars: &[char]) -> Expr {
    Expr::Quantified {
        expr: Box::new(Expr::CharacterRange(CharacterRange::Set {
            chars: chars.to_vec(),
            negated: true,
        })),
        quantifier: Quantifier::OneOrMore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grammar_builder() {
        let grammar = GrammarBuilder::new()
            .rule("root", seq(&[t("hello"), t(" "), nt("name")]))
            .rule("name", t("world"))
            .build();

        assert_eq!(grammar.declarations.len(), 2);
    }

    #[test]
    fn test_helpers() {
        // Test t()
        assert!(matches!(t("hello"), Expr::Characters(s) if s == "hello"));

        // Test nt()
        assert!(matches!(nt("rule"), Expr::NonTerminal(s) if s == "rule"));

        // Test nt_plus()
        assert!(matches!(
            nt_plus("item"),
            Expr::Quantified {
                quantifier: Quantifier::OneOrMore,
                ..
            }
        ));

        // Test t_star()
        assert!(matches!(
            t_star(" "),
            Expr::Quantified {
                quantifier: Quantifier::ZeroOrMore,
                ..
            }
        ));
    }

    #[test]
    fn test_not_chars() {
        let expr = not_chars(&['<', '>']);
        match expr {
            Expr::Quantified {
                expr: inner,
                quantifier: Quantifier::OneOrMore,
            } => match *inner {
                Expr::CharacterRange(CharacterRange::Set { chars, negated }) => {
                    assert!(negated);
                    assert_eq!(chars.len(), 2);
                    assert!(chars.contains(&'<'));
                    assert!(chars.contains(&'>'));
                }
                _ => panic!("Expected CharacterRange::Set"),
            },
            _ => panic!("Expected Quantified with OneOrMore"),
        }
    }

    #[test]
    fn test_grammar_output() {
        let grammar = GrammarBuilder::new()
            .rule("root", seq(&[t("hello"), t(" "), nt("name")]))
            .rule("name", t("world"))
            .build();

        let output = grammar.as_str();
        assert!(output.contains("root ::="));
        assert!(output.contains("name ::="));
        assert!(output.contains("\"hello\""));
        assert!(output.contains("\"world\""));
    }
}
