//! Builder pattern for constructing GBNF grammars programmatically.
//!
//! Provides a fluent interface and convenience functions for building grammars,
//! including safe composition of multiple grammars via [`GrammarBuilder::include_grammar_as`].
//!
//! Uses the typestate pattern to ensure at compile time that a root rule is
//! specified before building.

use std::marker::PhantomData;

use crate::compose::uniquify;
use crate::{CharacterRange, Expr, GbnfDeclaration, GbnfGrammar, Quantifier};

/// Typestate: no root has been specified yet. `build()` is not available.
pub struct NoRoot;
/// Typestate: a root has been specified. `build()` is available.
pub struct HasRoot;

/// Builder for constructing GBNF grammars.
///
/// Uses the typestate pattern: you must call `.root("name")` before `.build()`.
/// Calling `.build()` without `.root()` is a compile-time error.
///
/// # Example
/// ```ignore
/// use gbnf::builder::{GrammarBuilder, t, nt, seq};
///
/// let grammar = GrammarBuilder::new()
///     .rule("greeting", seq(&[t("Hello"), t(" "), nt("name")]))
///     .rule("name", t("World"))
///     .root("greeting")
///     .build();
/// ```
pub struct GrammarBuilder<State = NoRoot> {
    declarations: Vec<GbnfDeclaration>,
    root_name: String,
    _state: PhantomData<State>,
}

impl GrammarBuilder<NoRoot> {
    /// Create a new grammar builder with an empty grammar.
    pub fn new() -> Self {
        Self {
            declarations: Vec::new(),
            root_name: String::new(),
            _state: PhantomData,
        }
    }

    /// Create a grammar builder from an existing grammar.
    ///
    /// This allows you to add more rules to an existing grammar, which is useful
    /// when you have a base grammar (e.g., from JSON schema) and want to extend it.
    pub fn from_existing(grammar: GbnfGrammar) -> Self {
        Self {
            declarations: grammar.declarations,
            root_name: String::new(),
            _state: PhantomData,
        }
    }

    /// Set the root rule name, transitioning to the `HasRoot` state.
    ///
    /// The named rule must exist in the grammar when `build()` is called.
    pub fn root(self, name: &str) -> GrammarBuilder<HasRoot> {
        GrammarBuilder {
            declarations: self.declarations,
            root_name: name.to_string(),
            _state: PhantomData,
        }
    }
}

impl Default for GrammarBuilder<NoRoot> {
    fn default() -> Self {
        Self::new()
    }
}

// Methods available in both states.
impl<S> GrammarBuilder<S> {
    /// Add a rule to the grammar.
    pub fn rule(mut self, name: &str, expr: Expr) -> Self {
        self.declarations
            .push(GbnfDeclaration::new(name.to_string(), expr));
        self
    }

    /// Include an existing grammar, uniquifying its rules to avoid collisions,
    /// and create an alias for its root rule under the given name.
    ///
    /// This is the core composition operation. The included grammar's rules are
    /// all renamed with a unique suffix, and a new rule `alias ::= <uniquified_root>`
    /// is added so you can reference the included grammar by the alias name.
    ///
    /// Takes a reference so the same grammar can be included multiple times.
    ///
    /// # Panics
    ///
    /// Panics if the grammar has no declarations.
    pub fn include_grammar_as(mut self, grammar: &GbnfGrammar, alias: &str) -> Self {
        assert!(
            !grammar.declarations.is_empty(),
            "cannot include an empty grammar"
        );

        let uniquified = uniquify(grammar);

        // Add all declarations from the uniquified grammar
        self.declarations.extend(uniquified.declarations);

        // Add an alias rule: alias ::= uniquified_root
        self.declarations.push(GbnfDeclaration::new(
            alias.to_string(),
            Expr::NonTerminal(uniquified.root_name),
        ));

        self
    }
}

impl GrammarBuilder<HasRoot> {
    /// Build and return the final grammar.
    ///
    /// The root rule is placed first in the output (llama.cpp convention).
    pub fn build(self) -> GbnfGrammar {
        GbnfGrammar::new(self.declarations, self.root_name)
    }
}

/// Create a terminal expression (exact text match).
pub fn t(s: &str) -> Expr {
    Expr::Characters(s.to_string())
}

/// Create a non-terminal reference (references another rule exactly once).
pub fn nt(name: &str) -> Expr {
    Expr::NonTerminal(name.to_string())
}

/// Create a non-terminal reference with one-or-more repetition (`+`).
pub fn nt_plus(name: &str) -> Expr {
    Expr::Quantified {
        expr: Box::new(Expr::NonTerminal(name.to_string())),
        quantifier: Quantifier::OneOrMore,
    }
}

/// Create a non-terminal reference with zero-or-more repetition (`*`).
pub fn nt_star(name: &str) -> Expr {
    Expr::Quantified {
        expr: Box::new(Expr::NonTerminal(name.to_string())),
        quantifier: Quantifier::ZeroOrMore,
    }
}

/// Create a terminal with zero-or-more repetition (Kleene star).
pub fn t_star(s: &str) -> Expr {
    Expr::Quantified {
        expr: Box::new(Expr::Characters(s.to_string())),
        quantifier: Quantifier::ZeroOrMore,
    }
}

/// Create a sequence from multiple expressions.
pub fn seq(items: &[Expr]) -> Expr {
    if items.len() == 1 {
        items[0].clone()
    } else {
        Expr::Sequence(items.to_vec())
    }
}

/// Create an alternation from multiple expressions.
pub fn alt(items: &[Expr]) -> Expr {
    if items.len() == 1 {
        items[0].clone()
    } else {
        Expr::Alternation(items.to_vec())
    }
}

/// Create a character set that matches anything except the given characters.
/// Uses one-or-more repetition, so it must match at least one character.
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
    use crate::compose::reset_counter;

    #[test]
    fn test_grammar_builder() {
        let grammar = GrammarBuilder::new()
            .rule("greeting", seq(&[t("hello"), t(" "), nt("name")]))
            .rule("name", t("world"))
            .root("greeting")
            .build();

        assert_eq!(grammar.declarations.len(), 2);
        assert_eq!(grammar.root_name, "greeting");
    }

    #[test]
    fn test_helpers() {
        assert!(matches!(t("hello"), Expr::Characters(s) if s == "hello"));
        assert!(matches!(nt("rule"), Expr::NonTerminal(s) if s == "rule"));
        assert!(matches!(
            nt_plus("item"),
            Expr::Quantified {
                quantifier: Quantifier::OneOrMore,
                ..
            }
        ));
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
            .rule("greeting", seq(&[t("hello"), t(" "), nt("name")]))
            .rule("name", t("world"))
            .root("greeting")
            .build();

        let output = grammar.as_str();
        assert!(output.contains("greeting ::="));
        assert!(output.contains("name ::="));
        assert!(output.contains("\"hello\""));
        assert!(output.contains("\"world\""));
    }

    #[test]
    fn test_root_name_is_set() {
        let grammar = GrammarBuilder::new()
            .rule("helper", t("x"))
            .rule("myroot", nt("helper"))
            .root("myroot")
            .build();

        assert_eq!(grammar.root_name, "myroot");
    }

    #[test]
    fn test_include_grammar_as() {
        reset_counter();

        let inner = GbnfGrammar::new(
            vec![
                GbnfDeclaration::new("root".to_string(), Expr::NonTerminal("value".to_string())),
                GbnfDeclaration::new("value".to_string(), Expr::Characters("hello".to_string())),
            ],
            "root".to_string(),
        );

        let grammar = GrammarBuilder::new()
            .include_grammar_as(&inner, "greeting")
            .rule("root", seq(&[nt("greeting"), t(" world")]))
            .root("root")
            .build();

        assert_eq!(grammar.root_name, "root");

        // The uniquified inner rules and alias should be present
        let names: Vec<&str> = grammar
            .declarations
            .iter()
            .map(|d| d.name.as_str())
            .collect();
        assert!(names.contains(&"root-g0"));
        assert!(names.contains(&"value-g0"));
        assert!(names.contains(&"greeting"));
    }

    #[test]
    fn test_include_same_grammar_twice() {
        reset_counter();

        let inner = GbnfGrammar::new(
            vec![GbnfDeclaration::new(
                "root".to_string(),
                Expr::Characters("hello".to_string()),
            )],
            "root".to_string(),
        );

        let grammar = GrammarBuilder::new()
            .include_grammar_as(&inner, "first")
            .include_grammar_as(&inner, "second")
            .rule("root", seq(&[nt("first"), t(" "), nt("second")]))
            .root("root")
            .build();

        let names: Vec<&str> = grammar
            .declarations
            .iter()
            .map(|d| d.name.as_str())
            .collect();
        // Each inclusion gets a unique suffix
        assert!(names.contains(&"root-g0"));
        assert!(names.contains(&"root-g1"));
        assert!(names.contains(&"first"));
        assert!(names.contains(&"second"));
    }
}
