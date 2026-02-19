//! Grammar composition utilities.
//!
//! Provides functions for safely composing GBNF grammars by uniquifying
//! nonterminal identifiers to avoid naming collisions.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Expr, GbnfDeclaration, GbnfGrammar};

static GRAMMAR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a fresh unique suffix for grammar rule names.
fn next_suffix() -> String {
    let id = GRAMMAR_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("-g{}", id)
}

/// Recursively rename all `NonTerminal` references in an expression.
/// Names found in the map are replaced; others are left unchanged.
pub fn rename_nonterminals(expr: &Expr, map: &HashMap<String, String>) -> Expr {
    match expr {
        Expr::NonTerminal(name) => {
            Expr::NonTerminal(map.get(name).cloned().unwrap_or_else(|| name.clone()))
        }
        Expr::Group(inner) => Expr::Group(Box::new(rename_nonterminals(inner, map))),
        Expr::Sequence(items) => {
            Expr::Sequence(items.iter().map(|e| rename_nonterminals(e, map)).collect())
        }
        Expr::Alternation(alts) => {
            Expr::Alternation(alts.iter().map(|e| rename_nonterminals(e, map)).collect())
        }
        Expr::Quantified { expr, quantifier } => Expr::Quantified {
            expr: Box::new(rename_nonterminals(expr, map)),
            quantifier: quantifier.clone(),
        },
        // Characters, CharacterRange, Token have no nonterminal references
        other => other.clone(),
    }
}

/// Return a new grammar with all nonterminal names made globally unique.
///
/// Every rule name and every `NonTerminal` reference to those rules gets a
/// unique suffix appended (e.g., `-g0`, `-g1`). This makes it safe to merge
/// declarations from multiple grammars without naming collisions.
///
/// # Panics
///
/// Panics if the grammar's `root_name` does not match any declaration name.
pub fn uniquify(grammar: &GbnfGrammar) -> GbnfGrammar {
    let suffix = next_suffix();

    let rename_map: HashMap<String, String> = grammar
        .declarations
        .iter()
        .map(|d| (d.name.clone(), format!("{}{}", d.name, suffix)))
        .collect();

    let new_declarations = grammar
        .declarations
        .iter()
        .map(|d| {
            GbnfDeclaration::new(
                rename_map[&d.name].clone(),
                rename_nonterminals(&d.expr, &rename_map),
            )
        })
        .collect();

    let new_root = rename_map
        .get(&grammar.root_name)
        .unwrap_or_else(|| {
            panic!(
                "grammar root_name '{}' does not match any declaration",
                grammar.root_name
            )
        })
        .clone();

    GbnfGrammar::new(new_declarations, new_root)
}

/// Reset the grammar counter (for testing only).
#[cfg(test)]
pub fn reset_counter() {
    GRAMMAR_COUNTER.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CharacterRange, Quantifier};

    #[test]
    fn test_rename_nonterminals_simple() {
        let expr = Expr::NonTerminal("foo".to_string());
        let map = HashMap::from([("foo".to_string(), "foo-g0".to_string())]);
        let result = rename_nonterminals(&expr, &map);
        assert_eq!(result, Expr::NonTerminal("foo-g0".to_string()));
    }

    #[test]
    fn test_rename_nonterminals_leaves_unknown() {
        let expr = Expr::NonTerminal("unknown".to_string());
        let map = HashMap::from([("foo".to_string(), "foo-g0".to_string())]);
        let result = rename_nonterminals(&expr, &map);
        assert_eq!(result, Expr::NonTerminal("unknown".to_string()));
    }

    #[test]
    fn test_rename_nonterminals_recursive() {
        let expr = Expr::Sequence(vec![
            Expr::NonTerminal("a".to_string()),
            Expr::Group(Box::new(Expr::Alternation(vec![
                Expr::NonTerminal("b".to_string()),
                Expr::Characters("literal".to_string()),
            ]))),
            Expr::Quantified {
                expr: Box::new(Expr::NonTerminal("c".to_string())),
                quantifier: Quantifier::OneOrMore,
            },
        ]);

        let map = HashMap::from([
            ("a".to_string(), "a-g0".to_string()),
            ("b".to_string(), "b-g0".to_string()),
            ("c".to_string(), "c-g0".to_string()),
        ]);

        let result = rename_nonterminals(&expr, &map);
        let expected = Expr::Sequence(vec![
            Expr::NonTerminal("a-g0".to_string()),
            Expr::Group(Box::new(Expr::Alternation(vec![
                Expr::NonTerminal("b-g0".to_string()),
                Expr::Characters("literal".to_string()),
            ]))),
            Expr::Quantified {
                expr: Box::new(Expr::NonTerminal("c-g0".to_string())),
                quantifier: Quantifier::OneOrMore,
            },
        ]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_rename_preserves_non_nonterminals() {
        let expr = Expr::Sequence(vec![
            Expr::Characters("hello".to_string()),
            Expr::CharacterRange(CharacterRange::Range {
                begin: 'a',
                end: 'z',
                negated: false,
            }),
        ]);
        let map = HashMap::from([("foo".to_string(), "foo-g0".to_string())]);
        let result = rename_nonterminals(&expr, &map);
        assert_eq!(result, expr);
    }

    #[test]
    fn test_uniquify() {
        reset_counter();

        let grammar = GbnfGrammar::new(
            vec![
                GbnfDeclaration::new(
                    "root".to_string(),
                    Expr::NonTerminal("value".to_string()),
                ),
                GbnfDeclaration::new(
                    "value".to_string(),
                    Expr::Characters("hello".to_string()),
                ),
            ],
            "root".to_string(),
        );

        let uniquified = uniquify(&grammar);

        assert_eq!(uniquified.declarations.len(), 2);
        assert_eq!(uniquified.declarations[0].name, "root-g0");
        assert_eq!(uniquified.declarations[1].name, "value-g0");
        assert_eq!(uniquified.root_name, "root-g0");

        // The root rule should reference the uniquified value rule
        assert_eq!(
            uniquified.declarations[0].expr,
            Expr::NonTerminal("value-g0".to_string())
        );
    }

    #[test]
    fn test_uniquify_increments_counter() {
        reset_counter();

        let grammar = GbnfGrammar::new(
            vec![GbnfDeclaration::new(
                "root".to_string(),
                Expr::Characters("a".to_string()),
            )],
            "root".to_string(),
        );

        let u1 = uniquify(&grammar);
        let u2 = uniquify(&grammar);

        assert_eq!(u1.declarations[0].name, "root-g0");
        assert_eq!(u2.declarations[0].name, "root-g1");
    }
}
