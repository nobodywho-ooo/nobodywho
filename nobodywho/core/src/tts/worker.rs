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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    /// Test backend that records synthesis + voice-query calls and signals
    /// when it's been dropped (so the worker shutdown path is observable).
    struct MockBackend {
        synth_calls: Arc<AtomicUsize>,
        voice_calls: Arc<AtomicUsize>,
        dropped: Arc<AtomicBool>,
        voices: Vec<String>,
        next_pcm: Vec<f32>,
        sample_rate: u32,
    }

    impl Drop for MockBackend {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    impl TtsBackendImpl for MockBackend {
        fn synthesize_raw(&mut self, _text: &str) -> Result<(Vec<f32>, u32), TtsError> {
            self.synth_calls.fetch_add(1, Ordering::SeqCst);
            Ok((self.next_pcm.clone(), self.sample_rate))
        }

        fn available_voices(&self) -> Vec<String> {
            self.voice_calls.fetch_add(1, Ordering::SeqCst);
            self.voices.clone()
        }
    }

    struct MockHandles {
        synth_calls: Arc<AtomicUsize>,
        voice_calls: Arc<AtomicUsize>,
        dropped: Arc<AtomicBool>,
    }

    fn mock_with(voices: Vec<String>, pcm: Vec<f32>) -> (Box<dyn TtsBackendImpl>, MockHandles) {
        let synth_calls = Arc::new(AtomicUsize::new(0));
        let voice_calls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let backend = Box::new(MockBackend {
            synth_calls: Arc::clone(&synth_calls),
            voice_calls: Arc::clone(&voice_calls),
            dropped: Arc::clone(&dropped),
            voices,
            next_pcm: pcm,
            sample_rate: 16_000,
        });
        (
            backend,
            MockHandles {
                synth_calls,
                voice_calls,
                dropped,
            },
        )
    }

    #[test]
    fn synthesize_returns_riff_prefixed_wav() {
        let (backend, handles) = mock_with(vec![], vec![0.5, -0.5, 0.0]);
        let worker = TtsWorker::new(backend);
        let wav = worker.synthesize("hello".into()).unwrap();
        assert!(wav.starts_with(b"RIFF"));
        assert_eq!(handles.synth_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn available_voices_called_once_at_construction_then_cached() {
        let (backend, handles) = mock_with(vec!["alpha".into(), "beta".into()], vec![]);
        let worker = TtsWorker::new(backend);
        assert_eq!(worker.available_voices(), vec!["alpha", "beta"]);
        assert_eq!(worker.available_voices(), vec!["alpha", "beta"]);
        assert_eq!(handles.voice_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn multiple_concurrent_callers_all_complete() {
        let (backend, handles) = mock_with(vec![], vec![0.1; 8]);
        let worker = Arc::new(TtsWorker::new(backend));

        let threads: Vec<_> = (0..4)
            .map(|i| {
                let w = Arc::clone(&worker);
                thread::spawn(move || w.synthesize(format!("t{i}")).unwrap())
            })
            .collect();

        for t in threads {
            let wav = t.join().unwrap();
            assert!(wav.starts_with(b"RIFF"));
        }
        assert_eq!(handles.synth_calls.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn dropping_worker_terminates_thread_and_drops_backend() {
        let (backend, handles) = mock_with(vec![], vec![]);
        let worker = TtsWorker::new(backend);
        drop(worker);

        // The worker thread receives `Err` from `recv` once the sender drops,
        // exits the loop, and drops `backend` on its way out.
        for _ in 0..50 {
            if handles.dropped.load(Ordering::SeqCst) {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("backend was not dropped within 500ms after worker shutdown");
    }
}
