//! Internal worker implementation for chat operations.
//!
//! Contains the `ChatWorker` state and implementations for context management,
//! token generation, and chat session handling.

pub(crate) mod chat_worker;
pub(crate) mod context_management;
pub(crate) mod generation;

pub(crate) use chat_worker::ChatWorker;
