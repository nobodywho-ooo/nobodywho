use rubato::{FftFixedIn, Resampler};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CodecParameters, Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error::{DecodeError, IoError};
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

const RESAMPLE_CHUNK_SIZE: usize = 1024;

pub fn load_audio(path: &str, target_rate: u32) -> Result<Vec<f32>, String> {
    let mut format_reader = open_format_reader(path)?;
    let (track_id, sample_rate, n_channels, codec_params) =
        read_track_info(format_reader.as_ref(), path)?;
    let mut decoder = make_decoder(codec_params)?;
    let interleaved = collect_samples(&mut format_reader, &mut decoder, track_id)?;
    let mono = to_mono(interleaved, n_channels);

    if mono.is_empty() {
        return Err(format!("No audio samples decoded from '{}'", path));
    }

    if sample_rate == target_rate {
        Ok(mono)
    } else {
        resample(mono, sample_rate, target_rate)
    }
}

fn open_format_reader(path: &str) -> Result<Box<dyn FormatReader>, String> {
    let file =
        std::fs::File::open(path).map_err(|e| format!("Could not open '{}': {}", path, e))?;

    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
    {
        hint.with_extension(ext);
    }

    symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map(|p| p.format)
        .map_err(|e| format!("Could not probe format: {}", e))
}

fn read_track_info(
    format_reader: &dyn FormatReader,
    path: &str,
) -> Result<(u32, u32, usize, CodecParameters), String> {
    let track = format_reader
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| format!("No audio track in '{}'", path))?;

    let track_id = track.id;
    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| "Unknown sample rate".to_string())?;
    let n_channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(1);
    let codec_params = track.codec_params.clone();

    Ok((track_id, sample_rate, n_channels, codec_params))
}

fn make_decoder(codec_params: CodecParameters) -> Result<Box<dyn Decoder>, String> {
    symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .map_err(|e| format!("Could not create decoder: {}", e))
}

fn collect_samples(
    format_reader: &mut Box<dyn FormatReader>,
    decoder: &mut Box<dyn Decoder>,
    track_id: u32,
) -> Result<Vec<f32>, String> {
    let mut interleaved: Vec<f32> = Vec::new();

    loop {
        let packet = match format_reader.next_packet() {
            Ok(p) => p,
            Err(IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.to_string()),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(IoError(_) | DecodeError(_)) => continue,
            Err(e) => return Err(e.to_string()),
        };

        let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        sample_buf.copy_interleaved_ref(decoded);
        interleaved.extend_from_slice(sample_buf.samples());
    }

    Ok(interleaved)
}

fn to_mono(interleaved: Vec<f32>, n_channels: usize) -> Vec<f32> {
    if n_channels == 1 {
        return interleaved;
    }
    interleaved
        .chunks_exact(n_channels)
        .map(|frame| frame.iter().sum::<f32>() / n_channels as f32)
        .collect()
}

fn resample(samples: Vec<f32>, from_rate: u32, to_rate: u32) -> Result<Vec<f32>, String> {
    let mut resampler = FftFixedIn::<f32>::new(
        from_rate as usize,
        to_rate as usize,
        RESAMPLE_CHUNK_SIZE,
        2,
        1,
    )
    .map_err(|e| e.to_string())?;

    let mut output = Vec::new();

    let chunks = samples.chunks_exact(RESAMPLE_CHUNK_SIZE);
    let remainder = chunks.remainder();

    for chunk in chunks {
        let resampled = resampler
            .process(&[chunk], None)
            .map_err(|e| e.to_string())?;
        output.extend_from_slice(&resampled[0]);
    }

    if !remainder.is_empty() {
        let resampled = resampler
            .process_partial(Some(&[remainder]), None)
            .map_err(|e| e.to_string())?;
        output.extend_from_slice(&resampled[0]);
    }

    Ok(output)
}
