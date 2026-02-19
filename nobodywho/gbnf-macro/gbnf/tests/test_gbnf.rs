use gbnf::{GbnfGrammar, gbnf};

#[test]
fn test_basic_declarations() {
    let grammar: GbnfGrammar = gbnf! {
        root ::= foobar
        foobar ::= "Abekat"
        okabob ::= other
    };

    insta::assert_debug_snapshot!(grammar);
}

#[test]
fn test_single_declaration() {
    let grammar = gbnf! {
        single ::= value
    };

    insta::assert_debug_snapshot!(grammar);
}

#[test]
fn test_alternation() {
    let grammar = gbnf! {
        rule ::= foo | bar | baz
    };

    insta::assert_debug_snapshot!(grammar);
}

#[test]
fn test_sequence() {
    let grammar = gbnf! {
        rule ::= "hello" " " "world"
    };

    insta::assert_debug_snapshot!(grammar);
}

#[test]
fn test_quantifiers() {
    let grammar = gbnf! {
        rule ::= foo? bar+ baz*
    };

    insta::assert_debug_snapshot!(grammar);
}

#[test]
fn test_grouping() {
    let grammar = gbnf! {
        rule ::= (foo | bar) baz
    };

    insta::assert_debug_snapshot!(grammar);
}

#[test]
fn test_character_range() {
    let grammar = gbnf! {
        digit ::= [0-9]
        letter ::= [a-z]
    };

    insta::assert_debug_snapshot!(grammar);
}

#[test]
fn test_complex_example() {
    let grammar = gbnf! {
        root ::= chessmove+
        chessmove ::= (pawn | nonpawn | castle) [a-h]?
        pawn ::= [a-h] [1-8]
        nonpawn ::= [N-R]
        castle ::= "O-O" | "O-O-O"
    };

    insta::assert_debug_snapshot!(grammar);
}

#[test]
fn test_grammar_string_output() {
    let grammar = gbnf! {
        root ::= "hello" " " "world"
    };

    insta::assert_debug_snapshot!(grammar);
}

#[test]
fn test_ast_structure() {
    let grammar = gbnf! {
        greeting ::= "hello"
    };

    insta::assert_debug_snapshot!(grammar);
}

#[test]
fn test_grammar_composition() {
    let small_grammar = gbnf! {
        root ::= "hello"
    };

    let bigger_grammar = gbnf! {
        hello-grammar ::= {small_grammar}
        root ::= hello-grammar " " "world"
    };

    // Root should be first and named "root"
    assert_eq!(bigger_grammar.root_name, "root");
    assert_eq!(bigger_grammar.declarations[0].name, "root");

    // Should have 3 declarations: root, uniquified inner root, and the alias
    assert_eq!(bigger_grammar.declarations.len(), 3);

    // The alias "hello-grammar" should point to the uniquified root
    let alias = bigger_grammar
        .declarations
        .iter()
        .find(|d| d.name == "hello-grammar")
        .expect("should have hello-grammar alias");
    let alias_target = match &alias.expr {
        gbnf::Expr::NonTerminal(name) => name.clone(),
        _ => panic!("alias should be a NonTerminal reference"),
    };

    // The uniquified root should exist and contain "hello"
    let uniquified_root = bigger_grammar
        .declarations
        .iter()
        .find(|d| d.name == alias_target)
        .expect("uniquified root should exist");
    assert!(matches!(&uniquified_root.expr, gbnf::Expr::Characters(s) if s == "hello"));
}

#[test]
fn test_composition_double_include() {
    let inner = gbnf! {
        root ::= "x"
    };

    let grammar = gbnf! {
        first ::= {inner}
        second ::= {inner}
        root ::= first " " second
    };

    assert_eq!(grammar.root_name, "root");

    // Should have: root, 2 uniquified roots, 2 aliases = 5 declarations
    assert_eq!(grammar.declarations.len(), 5);

    // The two aliases should point to different uniquified names
    let first_alias = grammar
        .declarations
        .iter()
        .find(|d| d.name == "first")
        .unwrap();
    let second_alias = grammar
        .declarations
        .iter()
        .find(|d| d.name == "second")
        .unwrap();

    let first_target = match &first_alias.expr {
        gbnf::Expr::NonTerminal(name) => name.clone(),
        _ => panic!("expected NonTerminal"),
    };
    let second_target = match &second_alias.expr {
        gbnf::Expr::NonTerminal(name) => name.clone(),
        _ => panic!("expected NonTerminal"),
    };

    // Different unique suffixes
    assert_ne!(first_target, second_target);

    // Both targets should exist as declarations
    assert!(grammar.declarations.iter().any(|d| d.name == first_target));
    assert!(grammar
        .declarations
        .iter()
        .any(|d| d.name == second_target));
}

#[test]
fn test_composition_nested() {
    let inner = gbnf! {
        root ::= "x"
    };

    let middle = gbnf! {
        inner-part ::= {inner}
        root ::= inner-part "y"
    };

    let outer = gbnf! {
        middle-part ::= {middle}
        root ::= middle-part "z"
    };

    assert_eq!(outer.root_name, "root");
    assert_eq!(outer.declarations[0].name, "root");

    // Should have: root from outer, plus all uniquified middle rules
    // (which themselves contain uniquified inner rules), plus aliases
    // middle has: root, inner-uniquified-root, inner-part alias = 3 declarations
    // outer includes middle (uniquified: 3 decls) + middle-part alias + root = 5 declarations
    assert_eq!(outer.declarations.len(), 5);

    // The GBNF string should be valid (no empty names, no duplicates)
    let gbnf_str = outer.as_str();
    assert!(gbnf_str.contains("root ::="));
    assert!(gbnf_str.contains("middle-part"));
}

#[test]
fn test_composition_multi_rule_grammar() {
    let json_like = gbnf! {
        root ::= value
        value ::= "\"" chars "\""
        chars ::= [a-z]+
    };

    let grammar = gbnf! {
        json-val ::= {json_like}
        root ::= "<data>" json-val "</data>"
    };

    assert_eq!(grammar.root_name, "root");

    // json_like has 3 rules, so we get: 3 uniquified + 1 alias + 1 root = 5
    assert_eq!(grammar.declarations.len(), 5);

    // All declaration names should be unique
    let names: Vec<&str> = grammar
        .declarations
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    let unique_names: std::collections::HashSet<&str> = names.iter().copied().collect();
    assert_eq!(names.len(), unique_names.len(), "all names should be unique");
}
