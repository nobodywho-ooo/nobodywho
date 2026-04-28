use crate::errors::TtsError;
use crate::tts::backend::{synthesize_sync, TtsBackendImpl};
use std::sync::{mpsc, Mutex};

pub(super) struct TtsWorker {
    msg_tx: Mutex<mpsc::Sender<TtsMsg>>,
    available_voices: Vec<String>,
}

struct TtsMsg {
    text: String,
    response_tx: mpsc::Sender<Result<Vec<u8>, TtsError>>,
}

impl TtsWorker {
    pub fn new(mut backend: Box<dyn TtsBackendImpl>) -> Self {
        let available_voices = backend.available_voices();
        let (msg_tx, msg_rx) = mpsc::channel::<TtsMsg>();
        std::thread::spawn(move || {
            while let Ok(msg) = msg_rx.recv() {
                let _ = msg
                    .response_tx
                    .send(synthesize_sync(backend.as_mut(), &msg.text));
            }
        });

        Self {
            msg_tx: Mutex::new(msg_tx),
            available_voices,
        }
    }

    pub fn synthesize(&self, text: String) -> Result<Vec<u8>, TtsError> {
        let (response_tx, response_rx) = mpsc::channel();
        let guard = self
            .msg_tx
            .lock()
            .map_err(|e| TtsError::Synthesis(format!("tts worker lock poisoned: {e}")))?;
        guard
            .send(TtsMsg { text, response_tx })
            .map_err(|e| TtsError::Synthesis(format!("tts worker stopped: {e}")))?;
        drop(guard);
        response_rx
            .recv()
            .map_err(|e| TtsError::Synthesis(format!("tts worker stopped: {e}")))?
    }

    pub fn available_voices(&self) -> Vec<String> {
        self.available_voices.clone()
    }
}
