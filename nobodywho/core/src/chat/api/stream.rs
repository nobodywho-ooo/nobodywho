use crate::llm;

/// A stream of tokens from the model.
pub struct TokenStream {
    rx: tokio::sync::mpsc::Receiver<llm::WriteOutput>,
    completed_response: Option<String>,
}

impl TokenStream {
    pub(crate) fn new(rx: tokio::sync::mpsc::Receiver<llm::WriteOutput>) -> Self {
        Self {
            rx,
            completed_response: None,
        }
    }

    /// Get the next token from the stream.
    pub fn next_token(&mut self) -> Option<String> {
        if self.completed_response.is_some() {
            return None;
        }

        if let Some(output) = self.rx.blocking_recv() {
            match output {
                llm::WriteOutput::Token(token) => return Some(token),
                llm::WriteOutput::Done(completed_response) => {
                    self.completed_response = Some(completed_response);
                    return None;
                }
            }
        }
        None
    }

    /// Blocks until the  entire response is completed. Does not consume the response, so this
    /// method is idempotent.
    pub fn completed(&mut self) -> Result<String, crate::errors::CompletionError> {
        loop {
            match self.next_token() {
                Some(_) => {
                    continue;
                }
                None => {
                    return self
                        .completed_response
                        .clone()
                        .ok_or(crate::errors::CompletionError::WorkerCrashed);
                }
            }
        }
    }
}

/// A stream of tokens from the model, async version.
pub struct TokenStreamAsync {
    rx: tokio::sync::mpsc::Receiver<llm::WriteOutput>,
    completed_response: Option<String>,
}

impl TokenStreamAsync {
    pub fn new(rx: tokio::sync::mpsc::Receiver<llm::WriteOutput>) -> Self {
        Self {
            rx,
            completed_response: None,
        }
    }

    /// Waits for the next token in the stream. Consumes the token when emitted.
    pub async fn next_token(&mut self) -> Option<String> {
        if self.completed_response.is_some() {
            return None;
        }

        if let Some(output) = self.rx.recv().await {
            match output {
                llm::WriteOutput::Token(token) => return Some(token),
                llm::WriteOutput::Done(completed_response) => {
                    self.completed_response = Some(completed_response);
                    return None;
                }
            }
        }
        None
    }

    /// Waits for the entire response to be completed. Does not consume the response, so this
    /// method is idempotent.
    pub async fn completed(&mut self) -> Result<String, crate::errors::CompletionError> {
        loop {
            match self.next_token().await {
                Some(_) => {
                    continue;
                }
                None => {
                    return self
                        .completed_response
                        .clone()
                        .ok_or(crate::errors::CompletionError::WorkerCrashed);
                }
            }
        }
    }
}
