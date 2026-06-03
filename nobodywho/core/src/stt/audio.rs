//! Audio decoding and preprocessing for the STT pipeline.
//!
//! All functions are pure (no model I/O) and unit-testable without a GPU.

use crate::errors::SttError;
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub struct DecodedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl DecodedAudio {
    /// Decode an audio file (WAV / MP3 / FLAC) to mono f32 samples.
    ///
    /// Multi-channel audio is downmixed to mono by averaging all channels.
    pub fn from_file(path: &Path) -> Result<Self, SttError> {
        let mut format = probe_format(path)?;
        let mut info = find_track(format.as_mut())?;
        let samples = decode_packets(format.as_mut(), &mut info)?;
        Ok(Self {
            samples,
            sample_rate: info.sample_rate,
        })
    }

    /// Wrap raw i16 PCM samples (e.g. from a microphone stream) as `DecodedAudio`.
    pub fn from_pcm_i16(samples: &[i16], sample_rate: u32) -> Self {
        Self {
            samples: samples
                .iter()
                .map(|&s| s as f32 / i16::MAX as f32)
                .collect(),
            sample_rate,
        }
    }

    /// Split into consecutive 30-second windows, consuming self.
    ///
    /// Each window is exactly 480,000 samples (30s at 16 kHz), with the last
    /// one zero-padded. An empty input produces a single silent window.
    pub fn into_windows(self) -> Vec<Vec<f32>> {
        if self.samples.is_empty() {
            return vec![vec![0.0f32; 480_000]];
        }
        self.samples
            .chunks(480_000)
            .map(|chunk| {
                let mut window = vec![0.0f32; 480_000];
                window[..chunk.len()].copy_from_slice(chunk);
                window
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Internals — file decoding
// ---------------------------------------------------------------------------

struct TrackInfo {
    sample_rate: u32,
    n_channels: usize,
    track_id: u32,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
}

fn probe_format(path: &Path) -> Result<Box<dyn FormatReader>, SttError> {
    let file = std::fs::File::open(path)
        .map_err(|e| SttError::Audio(format!("open {}: {e}", path.display())))?;
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map(|probed| probed.format)
        .map_err(|e| SttError::Audio(format!("probe {}: {e}", path.display())))
}

fn find_track(format: &mut dyn FormatReader) -> Result<TrackInfo, SttError> {
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| SttError::Audio("no audio track found".into()))?;

    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| SttError::Audio("unknown sample rate".into()))?;
    let n_channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(1);
    let track_id = track.id;

    let decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| SttError::Audio(format!("decoder init: {e}")))?;

    Ok(TrackInfo {
        sample_rate,
        n_channels,
        track_id,
        decoder,
    })
}

/// Iterator over packets belonging to a specific track, hiding EOF and
/// wrong-track packets from the caller.
struct PacketIter<'a> {
    format: &'a mut dyn FormatReader,
    track_id: u32,
}

impl<'a> PacketIter<'a> {
    fn new(format: &'a mut dyn FormatReader, track_id: u32) -> Self {
        Self { format, track_id }
    }
}

impl Iterator for PacketIter<'_> {
    type Item = Result<symphonia::core::formats::Packet, SttError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.format.next_packet() {
                Ok(p) if p.track_id() == self.track_id => return Some(Ok(p)),
                Ok(_) => continue,
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return None
                }
                Err(e) => return Some(Err(SttError::Audio(format!("read packet: {e}")))),
            }
        }
    }
}

fn to_mono_samples(decoded: symphonia::core::audio::AudioBufferRef, n_channels: usize) -> Vec<f32> {
    let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
    buf.copy_interleaved_ref(decoded);
    buf.samples()
        .chunks_exact(n_channels)
        .map(|frame| frame.iter().sum::<f32>() / n_channels as f32)
        .collect()
}

fn decode_packets(
    format: &mut dyn FormatReader,
    info: &mut TrackInfo,
) -> Result<Vec<f32>, SttError> {
    let mut samples: Vec<f32> = Vec::new();
    for packet in PacketIter::new(format, info.track_id) {
        match info.decoder.decode(&packet?) {
            Ok(decoded) => samples.extend(to_mono_samples(decoded, info.n_channels)),
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(SttError::Audio(format!("decode: {e}"))),
        }
    }
    Ok(samples)
}

// ---------------------------------------------------------------------------
// Internals — resampling
// ---------------------------------------------------------------------------

pub struct AudioResampler {
    pub target_rate: u32,
    pub chunk_size: usize,
    pub sinc_params: SincInterpolationParameters,
}

impl Default for AudioResampler {
    fn default() -> Self {
        Self {
            target_rate: 16_000,
            chunk_size: 1024,
            sinc_params: SincInterpolationParameters {
                sinc_len: 256,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 256,
                window: WindowFunction::BlackmanHarris2,
            },
        }
    }
}

impl AudioResampler {
    pub fn resample(self, audio: DecodedAudio) -> Result<DecodedAudio, SttError> {
        if audio.sample_rate == self.target_rate {
            return Ok(audio);
        }
        let AudioResampler { target_rate, chunk_size, sinc_params } = self;
        let ratio = target_rate as f64 / audio.sample_rate as f64;
        let mut resampler = SincFixedIn::<f32>::new(ratio, 2.0, sinc_params, chunk_size, 1)
            .map_err(|e| SttError::Audio(format!("resampler init: {e}")))?;
        let output_capacity = (audio.samples.len() as f64 * ratio) as usize + chunk_size;
        let mut output = Vec::with_capacity(output_capacity);
        for chunk in audio.samples.chunks(chunk_size) {
            output.extend(Self::resample_chunk(&mut resampler, chunk, chunk_size)?);
        }
        Ok(DecodedAudio { samples: output, sample_rate: target_rate })
    }

    fn resample_chunk(resampler: &mut SincFixedIn<f32>, chunk: &[f32], chunk_size: usize) -> Result<Vec<f32>, SttError> {
        let mut padded = chunk.to_vec();
        padded.resize(chunk_size, 0.0);
        resampler
            .process(&[padded], None)
            .map(|mut waves| waves.remove(0))
            .map_err(|e| SttError::Audio(format!("resample: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW_SAMPLES: usize = 480_000;

    fn audio(samples: Vec<f32>) -> DecodedAudio {
        DecodedAudio { samples, sample_rate: 16_000 }
    }

    #[test]
    fn from_pcm_i16_scales_correctly() {
        let out = DecodedAudio::from_pcm_i16(&[0i16, i16::MAX, i16::MIN, 16383], 16000).samples;
        assert_eq!(out[0], 0.0);
        assert!((out[1] - 1.0).abs() < 1e-5, "MAX → 1.0, got {}", out[1]);
        assert!(out[2] < -0.999, "MIN → ≈-1.0, got {}", out[2]);
        assert!(
            (out[3] - 0.4999).abs() < 0.001,
            "16383 → ≈0.5, got {}",
            out[3]
        );
    }

    #[test]
    fn windows_exact_multiple() {
        let windows = audio(vec![1.0f32; WINDOW_SAMPLES * 2]).into_windows();
        assert_eq!(windows.len(), 2);
        assert!(windows.iter().all(|w| w.len() == WINDOW_SAMPLES));
    }

    #[test]
    fn windows_short_input_padded() {
        let windows = audio(vec![1.0f32; 1000]).into_windows();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].len(), WINDOW_SAMPLES);
        assert_eq!(&windows[0][..1000], &[1.0f32; 1000][..]);
        assert_eq!(windows[0][1000], 0.0);
    }

    #[test]
    fn windows_empty_input_produces_silent_window() {
        let windows = audio(vec![]).into_windows();
        assert_eq!(windows.len(), 1);
        assert!(windows[0].iter().all(|&s| s == 0.0));
    }

    #[test]
    fn windows_partial_last_chunk_padded() {
        let windows = audio(vec![2.0f32; WINDOW_SAMPLES + 500]).into_windows();
        assert_eq!(windows.len(), 2);
        assert!(windows[1][..500].iter().all(|&s| s == 2.0));
        assert!(windows[1][500..].iter().all(|&s| s == 0.0));
    }
}
