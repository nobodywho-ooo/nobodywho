//! GBNF Grammar types and macro
//!
//! This crate provides types for representing GBNF grammars and a macro for
//! parsing GBNF at compile time.

// Re-export all types from gbnf-core
pub use gbnf_core::{
    builder, compose, CharacterRange, Expr, GbnfDeclaration, GbnfGrammar, Quantifier, TokenRef,
};

// Re-export the macro
pub use gbnf_macro::gbnf;

// JSON Schema to GBNF conversion
pub mod json;
