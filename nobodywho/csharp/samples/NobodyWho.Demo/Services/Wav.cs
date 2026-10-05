using System.Buffers.Binary;

namespace NobodyWho.Demo.Services;

/// <summary>Reads a WAV file's audio as mono 16-bit samples, for voice activity detection.</summary>
public static class Wav
{
    public sealed record Audio(short[] Samples, int SampleRate)
    {
        public TimeSpan Duration => TimeSpan.FromSeconds((double)Samples.Length / SampleRate);
    }

    /// <summary>Decode 16-bit or 32-bit float PCM, mixing extra channels down to mono.</summary>
    /// <exception cref="FormatException">The bytes are not a WAV file in one of those formats.</exception>
    public static Audio Read(ReadOnlySpan<byte> wav)
    {
        if (wav.Length < 12 || !wav[..4].SequenceEqual("RIFF"u8) || !wav.Slice(8, 4).SequenceEqual("WAVE"u8))
            throw new FormatException("Not a WAV file.");

        int format = 0, channels = 0, sampleRate = 0, bits = 0;
        var position = 12;
        while (position + 8 <= wav.Length)
        {
            var id = wav.Slice(position, 4);
            var size = BinaryPrimitives.ReadInt32LittleEndian(wav.Slice(position + 4, 4));
            var body = wav.Slice(position + 8, Math.Min(size, wav.Length - position - 8));
            if (id.SequenceEqual("fmt "u8))
            {
                format = BinaryPrimitives.ReadUInt16LittleEndian(body);
                channels = BinaryPrimitives.ReadUInt16LittleEndian(body[2..]);
                sampleRate = BinaryPrimitives.ReadInt32LittleEndian(body[4..]);
                bits = BinaryPrimitives.ReadUInt16LittleEndian(body[14..]);
                if (format == 0xFFFE && body.Length >= 26)
                    format = BinaryPrimitives.ReadUInt16LittleEndian(body[24..]); // WAVE_FORMAT_EXTENSIBLE
            }
            else if (id.SequenceEqual("data"u8))
            {
                if (channels == 0)
                    throw new FormatException("WAV data comes before its format.");
                return new Audio(Decode(body, format, channels, bits), sampleRate);
            }
            position += 8 + size + (size & 1);
        }
        throw new FormatException("The WAV file has no audio data.");
    }

    private static short[] Decode(ReadOnlySpan<byte> data, int format, int channels, int bits)
    {
        var bytesPerSample = bits / 8;
        var frames = data.Length / (bytesPerSample * channels);
        var samples = new short[frames];
        for (var frame = 0; frame < frames; frame++)
        {
            double sum = 0;
            for (var channel = 0; channel < channels; channel++)
            {
                var at = data.Slice((frame * channels + channel) * bytesPerSample, bytesPerSample);
                sum += (format, bits) switch
                {
                    (1, 16) => BinaryPrimitives.ReadInt16LittleEndian(at) / 32768.0,
                    (3, 32) => BinaryPrimitives.ReadSingleLittleEndian(at),
                    _ => throw new FormatException($"Unsupported WAV encoding (format {format}, {bits}-bit)."),
                };
            }
            samples[frame] = (short)Math.Clamp(sum / channels * 32767.0, short.MinValue, short.MaxValue);
        }
        return samples;
    }
}
