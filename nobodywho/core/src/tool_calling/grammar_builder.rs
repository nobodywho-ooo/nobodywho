//! Helper utilities for building GBNF grammars with less boilerplate.
//!
//! This module provides a builder pattern and convenience functions for constructing
//! GBNF grammars programmatically, making it easier to create custom grammar rules
//! for constrained LLM output generation.

use gbnf::{
    CharacterSet, CharacterSetItem, Grammar, GrammarItem, NonTerminalSymbol, Production,
    ProductionItem, RepetitionType, Rule, TerminalSymbol,
};

/// Builder for constructing GBNF grammars.
///
/// Provides a fluent interface for adding rules and recurring rules to a grammar.
///
/// # Example
/// ```ignore
/// use crate::tool_calling::grammar_builder::{GrammarBuilder, t, nt};
///
/// let grammar = GrammarBuilder::new()
///     .rule("greeting", vec![t("Hello"), t(" "), nt("name")])
///     .rule("name", vec![t("World")])
///     .build();
/// ```
pub struct GrammarBuilder {
    grammar: Grammar,
}

impl GrammarBuilder {
    /// Create a new grammar builder with an empty grammar.
    pub fn new() -> Self {
        Self {
            grammar: Grammar::default(),
        }
    }

    /// Create a grammar builder from an existing grammar.
    ///
    /// This allows you to add more rules to an existing grammar, which is useful
    /// when you have a base grammar (e.g., from JSON schema) and want to extend it.
    ///
    /// # Example
    /// ```ignore
    /// let json_grammar = Grammar::from_json_schema(&schema)?;
    /// let extended = GrammarBuilder::from_existing(json_grammar)
    ///     .rule("wrapper", vec![t("<start>"), nt("root"), t("<end>")])
    ///     .build();
    /// ```
    pub fn from_existing(grammar: Grammar) -> Self {
        Self { grammar }
    }

    /// Add a rule to the grammar.
    ///
    /// Rules define productions in the grammar and are processed in order.
    pub fn rule(mut self, name: &str, items: Vec<ProductionItem>) -> Self {
        self.grammar.items.push(GrammarItem::Rule(Rule {
            lhs: NonTerminalSymbol { name: name.into() },
            rhs: Production { items },
        }));
        self
    }

    /// Add a recurring rule to the grammar.
    ///
    /// Recurring rules are stored separately in `grammar.recurring_items` and can be
    /// referenced multiple times without duplication.
    pub fn recurring_rule(mut self, name: &str, items: Vec<ProductionItem>) -> Self {
        self.grammar.recurring_items.insert(
            NonTerminalSymbol { name: name.into() },
            Production { items },
        );
        self
    }

    /// Build and return the final grammar.
    pub fn build(self) -> Grammar {
        self.grammar
    }
}

impl Default for GrammarBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a terminal production item (exact text match).
///
/// This matches the exact string provided with no repetition.
///
/// # Example
/// ```ignore
/// let item = t("hello"); // Matches exactly "hello"
/// ```
pub fn t(s: &str) -> ProductionItem {
    ProductionItem::Terminal(TerminalSymbol { value: s.into() }, RepetitionType::One)
}

/// Create a non-terminal reference.
///
/// This references another rule in the grammar with no repetition (exactly once).
///
/// # Example
/// ```ignore
/// let item = nt("my_rule"); // References the rule named "my_rule"
/// ```
pub fn nt(name: &str) -> ProductionItem {
    ProductionItem::NonTerminal(NonTerminalSymbol { name: name.into() }, RepetitionType::One)
}

/// Create a non-terminal reference with one-or-more repetition.
///
/// This references another rule that must occur at least once (like `+` in regex).
///
/// # Example
/// ```ignore
/// let item = nt_plus("item"); // Matches one or more "item"
/// ```
pub fn nt_plus(name: &str) -> ProductionItem {
    ProductionItem::NonTerminal(
        NonTerminalSymbol { name: name.into() },
        RepetitionType::OneOrMore,
    )
}

/// Create a terminal with zero-or-more repetition.
///
/// This matches the exact string zero or more times (Kleene star).
///
/// # Example
/// ```ignore
/// let item = t_star(" "); // Matches zero or more spaces
/// ```
pub fn t_star(s: &str) -> ProductionItem {
    ProductionItem::Terminal(
        TerminalSymbol { value: s.into() },
        RepetitionType::ZeroOrMore,
    )
}

/// Create a character set that matches anything except the given characters.
///
/// Uses one-or-more repetition, so it must match at least one character.
///
/// # Example
/// ```ignore
/// let item = not_chars(&['<', '>', '{', '}']); // Matches any char except < > { }
/// ```
pub fn not_chars(chars: &[char]) -> ProductionItem {
    ProductionItem::CharacterSet(
        CharacterSet {
            is_complement: true,
            items: chars
                .iter()
                .map(|&c| CharacterSetItem::Character(c))
                .collect(),
        },
        RepetitionType::OneOrMore,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grammar_builder() {
        let grammar = GrammarBuilder::new()
            .rule("root", vec![t("hello"), t(" "), nt("name")])
            .rule("name", vec![t("world")])
            .build();

        assert_eq!(grammar.items.len(), 2);
    }

    #[test]
    fn test_recurring_rule() {
        let grammar = GrammarBuilder::new()
            .recurring_rule("ws", vec![t_star(" ")])
            .rule("root", vec![nt("ws"), t("hello")])
            .build();

        assert_eq!(grammar.items.len(), 1);
        assert_eq!(grammar.recurring_items.len(), 1);
    }

    #[test]
    fn test_not_chars() {
        let item = not_chars(&['<', '>']);
        match item {
            ProductionItem::CharacterSet(charset, RepetitionType::OneOrMore) => {
                assert!(charset.is_complement);
                assert_eq!(charset.items.len(), 2);
            }
            _ => panic!("Expected CharacterSet with OneOrMore"),
        }
    }
}
