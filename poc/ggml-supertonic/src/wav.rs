use anyhow::{Context, Result};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

pub fn write_mono_16(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    let data_len = samples
        .len()
        .checked_mul(2)
        .and_then(|size| u32::try_from(size).ok())
        .context("WAV output exceeds the RIFF size limit")?;
    let riff_len = 36u32
        .checked_add(data_len)
        .context("WAV RIFF size overflow")?;
    let byte_rate = sample_rate
        .checked_mul(2)
        .context("WAV byte rate overflow")?;
    let mut writer = BufWriter::new(File::create(path)?);
    writer.write_all(b"RIFF")?;
    writer.write_all(&riff_len.to_le_bytes())?;
    writer.write_all(b"WAVEfmt ")?;
    writer.write_all(&16u32.to_le_bytes())?;
    writer.write_all(&1u16.to_le_bytes())?;
    writer.write_all(&1u16.to_le_bytes())?;
    writer.write_all(&sample_rate.to_le_bytes())?;
    writer.write_all(&byte_rate.to_le_bytes())?;
    writer.write_all(&2u16.to_le_bytes())?;
    writer.write_all(&16u16.to_le_bytes())?;
    writer.write_all(b"data")?;
    writer.write_all(&data_len.to_le_bytes())?;
    for &sample in samples {
        let sample = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer.write_all(&sample.to_le_bytes())?;
    }
    Ok(())
}
