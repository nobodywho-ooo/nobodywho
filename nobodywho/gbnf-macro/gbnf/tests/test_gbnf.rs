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
