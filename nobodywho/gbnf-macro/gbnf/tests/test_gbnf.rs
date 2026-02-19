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
        hello-grammar ::= @{small_grammar}
        root ::= hello-grammar " " "world"
    };

    assert_eq!(bigger_grammar.root_name, "root");

    // hello-grammar should resolve through to a uniquified root containing "hello"
    let alias = bigger_grammar
        .declarations
        .iter()
        .find(|d| d.name == "hello-grammar")
        .expect("should have hello-grammar declaration");
    let alias_target = match &alias.expr {
        gbnf::Expr::NonTerminal(name) => name.clone(),
        _ => panic!("hello-grammar should be a NonTerminal reference"),
    };

    // Follow the chain to find the uniquified root with "hello"
    fn resolve_to_characters(grammar: &GbnfGrammar, name: &str) -> Option<String> {
        let decl = grammar.declarations.iter().find(|d| d.name == name)?;
        match &decl.expr {
            gbnf::Expr::Characters(s) => Some(s.clone()),
            gbnf::Expr::NonTerminal(next) => resolve_to_characters(grammar, next),
            _ => None,
        }
    }
    assert_eq!(
        resolve_to_characters(&bigger_grammar, &alias_target),
        Some("hello".to_string())
    );
}

#[test]
fn test_composition_double_include() {
    let inner = gbnf! {
        root ::= "x"
    };

    let grammar = gbnf! {
        first ::= @{inner}
        second ::= @{inner}
        root ::= first " " second
    };

    assert_eq!(grammar.root_name, "root");

    // The two aliases should ultimately point to different uniquified names
    let first_decl = grammar
        .declarations
        .iter()
        .find(|d| d.name == "first")
        .unwrap();
    let second_decl = grammar
        .declarations
        .iter()
        .find(|d| d.name == "second")
        .unwrap();

    let first_target = match &first_decl.expr {
        gbnf::Expr::NonTerminal(name) => name.clone(),
        _ => panic!("expected NonTerminal"),
    };
    let second_target = match &second_decl.expr {
        gbnf::Expr::NonTerminal(name) => name.clone(),
        _ => panic!("expected NonTerminal"),
    };

    // Different unique suffixes (even if they go through intermediaries)
    assert_ne!(first_target, second_target);
}

#[test]
fn test_composition_nested() {
    let inner = gbnf! {
        root ::= "x"
    };

    let middle = gbnf! {
        inner-part ::= @{inner}
        root ::= inner-part "y"
    };

    let outer = gbnf! {
        middle-part ::= @{middle}
        root ::= middle-part "z"
    };

    assert_eq!(outer.root_name, "root");

    // The GBNF string should be valid (no empty names, no duplicates)
    let gbnf_str = outer.as_str();
    assert!(gbnf_str.contains("root ::="));
    assert!(gbnf_str.contains("middle-part"));

    // All declaration names should be unique
    let names: Vec<&str> = outer.declarations.iter().map(|d| d.name.as_str()).collect();
    let unique_names: std::collections::HashSet<&str> = names.iter().copied().collect();
    assert_eq!(
        names.len(),
        unique_names.len(),
        "all names should be unique"
    );
}

#[test]
fn test_composition_multi_rule_grammar() {
    let json_like = gbnf! {
        root ::= value
        value ::= "\"" chars "\""
        chars ::= [a-z]+
    };

    let grammar = gbnf! {
        json-val ::= @{json_like}
        root ::= "<data>" json-val "</data>"
    };

    assert_eq!(grammar.root_name, "root");

    // All declaration names should be unique
    let names: Vec<&str> = grammar
        .declarations
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    let unique_names: std::collections::HashSet<&str> = names.iter().copied().collect();
    assert_eq!(
        names.len(),
        unique_names.len(),
        "all names should be unique"
    );
}

#[test]
fn test_string_interpolation() {
    let tool_name = "sparklify";

    let grammar = gbnf! {
        root ::= "[TOOL_CALLS]" {tool_name} "[ARGS]"
    };

    // The interpolated string should appear as a Characters node
    let gbnf_str = grammar.as_str();
    assert!(gbnf_str.contains("\"sparklify\""));
    assert!(gbnf_str.contains("\"[TOOL_CALLS]\""));
    assert!(gbnf_str.contains("\"[ARGS]\""));
}

#[test]
fn test_string_interpolation_in_alternation() {
    let option_a = "foo";
    let option_b = "bar";

    let grammar = gbnf! {
        root ::= {option_a} | {option_b} | "baz"
    };

    let gbnf_str = grammar.as_str();
    assert!(gbnf_str.contains("\"foo\""));
    assert!(gbnf_str.contains("\"bar\""));
    assert!(gbnf_str.contains("\"baz\""));
}

#[test]
fn test_mixed_interpolation_and_inclusion() {
    let tool_name = "sparklify";
    let inner_grammar = gbnf! {
        root ::= [a-z]+
    };

    let grammar = gbnf! {
        args ::= @{inner_grammar}
        root ::= "[TOOL_CALLS]" {tool_name} "[ARGS]" args
    };

    assert_eq!(grammar.root_name, "root");
    let gbnf_str = grammar.as_str();
    assert!(gbnf_str.contains("\"sparklify\""));
    assert!(gbnf_str.contains("\"[TOOL_CALLS]\""));
    assert!(gbnf_str.contains("\"[ARGS]\""));
    assert!(gbnf_str.contains("args"));
}

#[test]
fn test_inline_grammar_inclusion() {
    let inner = gbnf! {
        root ::= [a-z]+
    };

    // @{} in expression position (not entire RHS)
    let grammar = gbnf! {
        root ::= "prefix-" @{inner} "-suffix"
    };

    assert_eq!(grammar.root_name, "root");
    let gbnf_str = grammar.as_str();
    assert!(gbnf_str.contains("\"prefix-\""));
    assert!(gbnf_str.contains("\"-suffix\""));
}
