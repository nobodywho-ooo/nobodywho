//! Public-facing API types for chat interactions.
//!
//! This module contains all user-facing types including message structures, configuration,
//! handles for chat interaction, and streaming response types.

pub mod handle;
pub mod message;
pub mod stream;

pub use handle::{ChatBuilder, ChatConfig, ChatHandle, ChatHandleAsync};
pub use message::{Message, Role};
pub use stream::{TokenStream, TokenStreamAsync};
