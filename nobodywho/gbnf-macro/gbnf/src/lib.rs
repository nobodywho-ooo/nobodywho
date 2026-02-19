//! GBNF Grammar types and macro
//!
//! This crate provides types for representing GBNF grammars and a macro for
//! parsing GBNF at compile time.

// Re-export all types from gbnf-types
pub use gbnf_types::{CharacterRange, Expr, GbnfDeclaration, GbnfGrammar, Quantifier, TokenRef};

// Re-export the macro
pub use gbnf_macro::gbnf;

// JSON Schema to GBNF conversion
pub mod json;

// Grammar builder and composition utilities
pub mod builder;
pub mod compose;
