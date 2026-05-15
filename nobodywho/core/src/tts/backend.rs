use crate::errors::TtsError;
use crate::tts::{backends, TtsConfig, TtsDevice};
use std::sync::{mpsc, Mutex};
use std::time::Instant;
use tracing::info;

pub(super) trait TtsBackendImpl: Send {
    fn synthesize_raw(&mut self, text: &str) -> Result<(Vec<f32>, u32), TtsError>;
}

pub(super) fn load_backend(
    config: TtsConfig,
    device: TtsDevice,
) -> Result<Box<dyn TtsBackendImpl>, TtsError> {
    match config {
        TtsConfig::Kokoro(config) => {
            let init_start = Instant::now();
            let backend = backends::KokoroBackend::new(
                &config.model_dir,
                config.voice,
                config.language,
                config.speed,
                config.sample_rate,
                device,
            )?;
            info!(elapsed = ?init_start.elapsed(), "Initialized Kokoro TTS");
            Ok(Box::new(backend))
        }
        TtsConfig::Piper(config) => {
            let init_start = Instant::now();
            let backend =
                backends::PiperBackend::new(&config.model_dir, config.speaker_id, device)?;
            info!(elapsed = ?init_start.elapsed(), "Initialized Piper TTS");
            Ok(Box::new(backend))
        }
        TtsConfig::Chatterbox(config) => {
            let init_start = Instant::now();
            let backend = backends::ChatterboxBackend::new(
                &config.model_dir,
                config.reference_wav.as_deref(),
                config.language,
                config.sampling,
                config.sample_rate,
                device,
            )?;
            info!(elapsed = ?init_start.elapsed(), "Initialized Chatterbox TTS");
            Ok(Box::new(backend))
        }
        TtsConfig::Roest(config) => {
            let init_start = Instant::now();
            let backend = backends::RoestBackend::new(
                &config.model_dir,
                config.language,
                config.sampling,
                config.sample_rate,
                device,
            )?;
            info!(elapsed = ?init_start.elapsed(), "Initialized Røst TTS");
            Ok(Box::new(backend))
        }
    }
}

pub(super) fn synthesize_sync(
    backend: &mut dyn TtsBackendImpl,
    text: &str,
) -> Result<Vec<u8>, TtsError> {
    let synth_start = Instant::now();
    let (samples, sample_rate) = backend.synthesize_raw(text)?;

    info!(
        n_samples = samples.len(),
        duration_secs = samples.len() as f32 / sample_rate as f32,
        elapsed = ?synth_start.elapsed(),
        "Synthesized audio"
    );

    encode_wav(&samples, sample_rate)
}

fn encode_wav(pcm: &[f32], sample_rate: u32) -> Result<Vec<u8>, TtsError> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut buffer = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buffer);
        let mut writer = hound::WavWriter::new(cursor, spec)
            .map_err(|e| TtsError::WavEncoding(e.to_string()))?;

        for &sample in pcm {
            let s = (sample * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            writer
                .write_sample(s)
                .map_err(|e| TtsError::WavEncoding(e.to_string()))?;
        }

        writer
            .finalize()
            .map_err(|e| TtsError::WavEncoding(e.to_string()))?;
    }

    Ok(buffer)
}

pub(super) struct TtsWorker {
    msg_tx: Mutex<mpsc::Sender<TtsMsg>>,
}

struct TtsMsg {
    text: String,
    response_tx: mpsc::Sender<Result<Vec<u8>, TtsError>>,
}

impl TtsWorker {
    pub fn new(mut backend: Box<dyn TtsBackendImpl>) -> Self {
        let (msg_tx, msg_rx) = mpsc::channel::<TtsMsg>();
        std::thread::spawn(move || {
            while let Ok(msg) = msg_rx.recv() {
                if msg
                    .response_tx
                    .send(synthesize_sync(backend.as_mut(), &msg.text))
                    .is_err()
                {
                    tracing::warn!("TTS caller dropped before result could be delivered");
                }
            }
        });

        Self {
            msg_tx: Mutex::new(msg_tx),
        }
    }

    pub fn synthesize(&self, text: String) -> Result<Vec<u8>, TtsError> {
        let (response_tx, response_rx) = mpsc::channel();
        self.msg_tx
            .lock()
            .map_err(|e| TtsError::Synthesis(format!("tts worker lock poisoned: {e}")))?
            .send(TtsMsg { text, response_tx })
            .map_err(|e| TtsError::Synthesis(format!("tts worker stopped: {e}")))?;
        response_rx
            .recv()
            .map_err(|e| TtsError::Synthesis(format!("tts worker stopped: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

    struct MockBackend {
        synth_calls: Arc<AtomicUsize>,
        next_pcm: Vec<f32>,
        sample_rate: u32,
    }

    impl TtsBackendImpl for MockBackend {
        fn synthesize_raw(&mut self, _text: &str) -> Result<(Vec<f32>, u32), TtsError> {
            self.synth_calls.fetch_add(1, Ordering::SeqCst);
            Ok((self.next_pcm.clone(), self.sample_rate))
        }
    }

    struct MockHandles {
        synth_calls: Arc<AtomicUsize>,
    }

    fn mock_with(pcm: Vec<f32>) -> (Box<dyn TtsBackendImpl>, MockHandles) {
        let synth_calls = Arc::new(AtomicUsize::new(0));
        let backend = Box::new(MockBackend {
            synth_calls: Arc::clone(&synth_calls),
            next_pcm: pcm,
            sample_rate: 16_000,
        });
        (backend, MockHandles { synth_calls })
    }

    #[test]
    fn multiple_concurrent_callers_all_complete() {
        let (backend, handles) = mock_with(vec![0.1; 8]);
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
}
