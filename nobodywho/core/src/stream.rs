//! Generic token streaming types shared by the LLM and STT modules.

use tokio::sync::mpsc::UnboundedReceiver;

/// A single item on a token stream.
pub enum StreamOutput<E> {
    /// One decoded token piece, emitted as it is generated.
    Token(String),
    /// Generation finished; carries the full clean output.
    Done(String),
    /// An error occurred during generation.
    Error(E),
}

/// Blocking token stream. Call [`next_token`](Self::next_token) to drive
/// token-by-token, or [`completed`](Self::completed) to collect the full text.
pub struct TokenStream<E> {
    pub(crate) rx: UnboundedReceiver<StreamOutput<E>>,
    pub(crate) done: Option<String>,
}

impl<E> TokenStream<E> {
    pub fn new(rx: UnboundedReceiver<StreamOutput<E>>) -> Self {
        Self { rx, done: None }
    }

    /// Return the next token piece, or `None` when generation is finished.
    pub fn next_token(&mut self) -> Result<Option<String>, E> {
        if self.done.is_some() {
            return Ok(None);
        }
        match self.rx.blocking_recv() {
            Some(StreamOutput::Token(t)) => Ok(Some(t)),
            Some(StreamOutput::Done(text)) => { self.done = Some(text); Ok(None) }
            Some(StreamOutput::Error(e)) => Err(e),
            None => Ok(None),
        }
    }

    /// Drain all tokens and return the full output text.
    pub fn completed(&mut self) -> Result<String, E> {
        loop {
            match self.next_token()? {
                Some(_) => continue,
                None => return Ok(self.done.clone().unwrap_or_default()),
            }
        }
    }
}

/// Async token stream.
pub struct TokenStreamAsync<E> {
    pub(crate) rx: UnboundedReceiver<StreamOutput<E>>,
    pub(crate) done: Option<String>,
}

impl<E> TokenStreamAsync<E> {
    pub fn new(rx: UnboundedReceiver<StreamOutput<E>>) -> Self {
        Self { rx, done: None }
    }

    pub async fn next_token(&mut self) -> Result<Option<String>, E> {
        if self.done.is_some() {
            return Ok(None);
        }
        match self.rx.recv().await {
            Some(StreamOutput::Token(t)) => Ok(Some(t)),
            Some(StreamOutput::Done(text)) => { self.done = Some(text); Ok(None) }
            Some(StreamOutput::Error(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub async fn completed(&mut self) -> Result<String, E> {
        loop {
            match self.next_token().await? {
                Some(_) => continue,
                None => return Ok(self.done.clone().unwrap_or_default()),
            }
        }
    }
}
