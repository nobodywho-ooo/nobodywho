//! Builder pattern for constructing GBNF grammars programmatically.
//!
//! Provides a fluent interface and convenience functions for building grammars,
//! including safe composition of multiple grammars via [`GrammarBuilder::include_grammar_as`].

use crate::compose::uniquify;
use crate::{CharacterRange, Expr, GbnfDeclaration, GbnfGrammar, Quantifier};

/// Builder for constructing GBNF grammars.
///
/// Provides a fluent interface for adding rules to a grammar, and for
/// safely composing multiple grammars together.
///
/// # Example
/// ```ignore
/// use gbnf::builder::{GrammarBuilder, t, nt, seq};
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
    ///
    /// # Example
    /// ```ignore
    /// let json_grammar = json_schema_to_grammar(&schema, "root")?;
    ///
    /// let grammar = GrammarBuilder::new()
    ///     .include_grammar_as(&json_grammar, "json-args")
    ///     .rule("root", seq(&[t("<tool>"), nt("json-args"), t("</tool>")]))
    ///     .build();
    /// ```
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

    /// Build and return the final grammar.
    ///
    /// If a rule named `"root"` exists, it is moved to position 0 to serve
    /// as the GBNF start symbol (llama.cpp convention).
    pub fn build(mut self) -> GbnfGrammar {
        if let Some(pos) = self.declarations.iter().position(|d| d.name == "root") {
            let root_decl = self.declarations.remove(pos);
            self.declarations.insert(0, root_decl);
        }
        GbnfGrammar::new(self.declarations)
    }
}

impl Default for GrammarBuilder {
    fn default() -> Self {
        Self::new()
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
            .rule("root", seq(&[t("hello"), t(" "), nt("name")]))
            .rule("name", t("world"))
            .build();

        assert_eq!(grammar.declarations.len(), 2);
        assert_eq!(grammar.root_name, "root");
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
            .rule("root", seq(&[t("hello"), t(" "), nt("name")]))
            .rule("name", t("world"))
            .build();

        let output = grammar.as_str();
        assert!(output.contains("root ::="));
        assert!(output.contains("name ::="));
        assert!(output.contains("\"hello\""));
        assert!(output.contains("\"world\""));
    }

    #[test]
    fn test_build_reorders_root() {
        let grammar = GrammarBuilder::new()
            .rule("helper", t("x"))
            .rule("root", nt("helper"))
            .build();

        assert_eq!(grammar.declarations[0].name, "root");
        assert_eq!(grammar.declarations[1].name, "helper");
        assert_eq!(grammar.root_name, "root");
    }

    #[test]
    fn test_include_grammar_as() {
        reset_counter();

        let inner = GbnfGrammar::new(vec![
            GbnfDeclaration::new("root".to_string(), Expr::NonTerminal("value".to_string())),
            GbnfDeclaration::new("value".to_string(), Expr::Characters("hello".to_string())),
        ]);

        let grammar = GrammarBuilder::new()
            .include_grammar_as(&inner, "greeting")
            .rule("root", seq(&[nt("greeting"), t(" world")]))
            .build();

        // root should be first (reordered by build)
        assert_eq!(grammar.declarations[0].name, "root");
        assert_eq!(grammar.root_name, "root");

        // The uniquified inner rules and alias should be present
        let names: Vec<&str> = grammar.declarations.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"root-g0"));
        assert!(names.contains(&"value-g0"));
        assert!(names.contains(&"greeting"));
    }

    #[test]
    fn test_include_same_grammar_twice() {
        reset_counter();

        let inner = GbnfGrammar::new(vec![GbnfDeclaration::new(
            "root".to_string(),
            Expr::Characters("hello".to_string()),
        )]);

        let grammar = GrammarBuilder::new()
            .include_grammar_as(&inner, "first")
            .include_grammar_as(&inner, "second")
            .rule("root", seq(&[nt("first"), t(" "), nt("second")]))
            .build();

        let names: Vec<&str> = grammar.declarations.iter().map(|d| d.name.as_str()).collect();
        // Each inclusion gets a unique suffix
        assert!(names.contains(&"root-g0"));
        assert!(names.contains(&"root-g1"));
        assert!(names.contains(&"first"));
        assert!(names.contains(&"second"));
    }
}
