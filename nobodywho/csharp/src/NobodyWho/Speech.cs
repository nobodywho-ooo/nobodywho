using System;
using System.Threading;
using System.Threading.Tasks;

namespace NobodyWho;

/// <summary>A text-to-speech model architecture.</summary>
public enum TextToSpeechArchitecture
{
    /// <summary>Kokoro.</summary>
    Kokoro,

    /// <summary>Pocket TTS.</summary>
    PocketTts,

    /// <summary>Supertonic.</summary>
    Supertonic,
}

/// <summary>Where an ONNX model (text-to-speech, voice activity detection) runs.</summary>
public enum InferenceDevice
{
    /// <summary>CUDA when available, otherwise the CPU.</summary>
    Auto,

    /// <summary>The CPU.</summary>
    Cpu,

    /// <summary>An NVIDIA GPU through CUDA.</summary>
    Cuda,
}

internal static class SpeechNames
{
    internal static string? Of(TextToSpeechArchitecture? architecture) => architecture switch
    {
        null => null,
        TextToSpeechArchitecture.Kokoro => "kokoro",
        TextToSpeechArchitecture.PocketTts => "pocket-tts",
        TextToSpeechArchitecture.Supertonic => "supertonic",
        _ => throw new ArgumentOutOfRangeException(nameof(architecture), architecture, null),
    };

    internal static string Of(InferenceDevice device) => device switch
    {
        InferenceDevice.Auto => "auto",
        InferenceDevice.Cpu => "cpu",
        InferenceDevice.Cuda => "cuda",
        _ => throw new ArgumentOutOfRangeException(nameof(device), device, null),
    };
}

/// <summary>Synthesizes speech from text, as WAV audio.</summary>
/// <example>
/// <code>
/// using var tts = await TextToSpeech.LoadAsync("hf://onnx-community/Kokoro-82M-v1.0-ONNX", voice: "af_heart");
/// byte[] wav = await tts.SynthesizeAsync("Hello from NobodyWho!");
/// File.WriteAllBytes("hello.wav", wav);
/// </code>
/// </example>
public sealed class TextToSpeech : IDisposable
{
    private readonly Native.RustTextToSpeech _inner;

    private TextToSpeech(Native.RustTextToSpeech inner) => _inner = inner;

    /// <summary>
    /// Load a text-to-speech model synchronously. Prefer <see cref="LoadAsync"/>, which does not
    /// block the calling thread while the model downloads and loads.
    /// </summary>
    /// <inheritdoc cref="LoadAsync" path="/param"/>
    public TextToSpeech(
        string source,
        TextToSpeechArchitecture? architecture = null,
        string? voice = null,
        string? language = null,
        float? speed = null,
        int? steps = null,
        float? silenceDuration = null,
        string? precision = null,
        float? temperature = null,
        string? huggingFaceToken = null,
        InferenceDevice device = InferenceDevice.Auto)
    {
        ArgumentNullException.ThrowIfNull(source);
        _inner = Checked.Call(() => new Native.RustTextToSpeech(
            source,
            SpeechNames.Of(architecture),
            voice,
            language,
            speed,
            Checked.ToUInt(steps, nameof(steps)),
            silenceDuration,
            precision,
            temperature,
            huggingFaceToken,
            SpeechNames.Of(device)));
    }

    /// <summary>Load a text-to-speech model.</summary>
    /// <param name="source">A Hugging Face repo (<c>hf://owner/repo</c>) or a local directory.</param>
    /// <param name="architecture">The model's architecture. Required when it cannot be told from <paramref name="source"/>.</param>
    /// <param name="voice">The voice to speak with. Voices depend on the model.</param>
    /// <param name="language">The language to speak.</param>
    /// <param name="speed">Speaking speed, where 1 is normal (Kokoro, Supertonic).</param>
    /// <param name="steps">Denoising steps: more is slower and cleaner (Pocket TTS, Supertonic).</param>
    /// <param name="silenceDuration">Seconds of silence between sentences (Supertonic).</param>
    /// <param name="precision"><c>int8</c> or <c>fp32</c> (Pocket TTS).</param>
    /// <param name="temperature">Sampling temperature (Pocket TTS).</param>
    /// <param name="huggingFaceToken">Token for downloading gated voices (Pocket TTS).</param>
    /// <param name="device">Where the model runs.</param>
    /// <param name="cancellationToken">Stops waiting for the load. The load keeps running and its result is freed.</param>
    public static async Task<TextToSpeech> LoadAsync(
        string source,
        TextToSpeechArchitecture? architecture = null,
        string? voice = null,
        string? language = null,
        float? speed = null,
        int? steps = null,
        float? silenceDuration = null,
        string? precision = null,
        float? temperature = null,
        string? huggingFaceToken = null,
        InferenceDevice device = InferenceDevice.Auto,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(source);
        var load = Checked.CallAsync(() => Native.NativeMethods.LoadTextToSpeech(
            source,
            SpeechNames.Of(architecture),
            voice,
            language,
            speed,
            Checked.ToUInt(steps, nameof(steps)),
            silenceDuration,
            precision,
            temperature,
            huggingFaceToken,
            SpeechNames.Of(device)));
        return new TextToSpeech(await Cancellation.WaitOrDispose(load, cancellationToken).ConfigureAwait(false));
    }

    /// <summary>Synthesize <paramref name="text"/> and return WAV bytes.</summary>
    public Task<byte[]> SynthesizeAsync(string text) => Checked.CallAsync(() => _inner.SynthesizeAsync(text));

    /// <summary>Synthesize <paramref name="text"/> on the calling thread and return WAV bytes.</summary>
    public byte[] Synthesize(string text) => Checked.Call(() => _inner.Synthesize(text));

    /// <summary>Free the underlying native resources.</summary>
    public void Dispose() => _inner.Dispose();
}

/// <summary>Transcribes speech with Whisper models in ONNX format.</summary>
/// <example>
/// <code>
/// using var stt = await SpeechToText.LoadAsync("hf://onnx-community/whisper-base");
/// string transcript = await stt.TranscribeFile("recording.mp3").CompletedAsync();
/// </code>
/// </example>
public sealed class SpeechToText : IDisposable
{
    private readonly Native.RustSpeechToText _inner;

    private SpeechToText(Native.RustSpeechToText inner) => _inner = inner;

    /// <summary>Load a Whisper model.</summary>
    /// <param name="source">A Hugging Face repo (<c>hf://owner/repo</c>) or a local directory.</param>
    /// <param name="language">An ISO 639-1 code such as <c>en</c>. <c>null</c> detects the language.</param>
    /// <param name="quantization">
    /// The ONNX variant to load: <c>default</c>, <c>fp16</c>, <c>int8</c>, <c>uint8</c>,
    /// <c>bnb4</c>, <c>q4</c>, <c>q4f16</c> or <c>quantized</c>. <c>null</c> uses <c>default</c>.
    /// </param>
    /// <param name="cancellationToken">Stops waiting for the load. The load keeps running and its result is freed.</param>
    public static async Task<SpeechToText> LoadAsync(
        string source,
        string? language = null,
        string? quantization = null,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(source);
        var load = Checked.CallAsync(() => Native.NativeMethods.LoadSpeechToText(source, language, quantization));
        return new SpeechToText(await Cancellation.WaitOrDispose(load, cancellationToken).ConfigureAwait(false));
    }

    /// <summary>Start transcribing an audio file (WAV or MP3).</summary>
    public SpeechToTextStream TranscribeFile(string path) =>
        new(Checked.Call(() => _inner.TranscribeFile(path)));

    /// <summary>Start transcribing 16-bit PCM samples, such as from a microphone.</summary>
    /// <param name="samples">Mono 16-bit samples.</param>
    /// <param name="sampleRate">The capture rate in Hz. Audio is resampled to 16 kHz internally.</param>
    public SpeechToTextStream TranscribePcm(short[] samples, int sampleRate)
    {
        ArgumentNullException.ThrowIfNull(samples);
        return new(Checked.Call(() => _inner.TranscribePcm(samples, Checked.ToUInt(sampleRate, nameof(sampleRate)))));
    }

    /// <summary>Free the underlying native resources.</summary>
    public void Dispose() => _inner.Dispose();
}

/// <summary>The confirmed state returned by <see cref="VoiceActivityDetection.Push"/>.</summary>
public enum VoiceActivityDetectionEvent
{
    /// <summary>Speech continues.</summary>
    Speech,

    /// <summary>Speech was just confirmed to have started.</summary>
    SpeechStarted,

    /// <summary>Speech was just confirmed to have ended. Call <see cref="VoiceActivityDetection.Finish"/>.</summary>
    SpeechEnded,

    /// <summary>Silence continues.</summary>
    Silence,
}

/// <summary>
/// Detects speech in live, streaming audio with Silero VAD.
/// </summary>
/// <remarks>
/// Feed each new chunk of audio to <see cref="Push"/> as it arrives. The detector buffers the
/// current turn itself, with a short pre-roll so the start of speech is not clipped. Once
/// <see cref="Push"/> returns <see cref="VoiceActivityDetectionEvent.SpeechEnded"/>, call
/// <see cref="Finish"/> to get that turn's audio and reset for the next one.
/// </remarks>
/// <example>
/// <code>
/// using var vad = await VoiceActivityDetection.LoadAsync(sampleRate: 16000);
/// if (vad.Push(chunk) == VoiceActivityDetectionEvent.SpeechEnded)
/// {
///     short[] turn = vad.Finish();
/// }
/// </code>
/// </example>
public sealed class VoiceActivityDetection : IDisposable
{
    private readonly Native.RustVoiceActivityDetection _inner;

    private VoiceActivityDetection(Native.RustVoiceActivityDetection inner) => _inner = inner;

    /// <summary>Load a voice activity detector.</summary>
    /// <param name="sampleRate">The rate of the audio passed to <see cref="Push"/>. Audio is resampled to 16 kHz internally.</param>
    /// <param name="source">
    /// A Hugging Face repo (<c>hf://owner/repo</c>) or local directory holding the Silero VAD
    /// model. <c>null</c> uses <c>hf://onnx-community/silero-vad</c>.
    /// </param>
    /// <param name="threshold">Speech probability above which audio counts as speech.</param>
    /// <param name="minSilenceDurationMs">Silence needed to confirm that speech ended.</param>
    /// <param name="minSpeechDurationMs">Speech needed to confirm that speech started.</param>
    /// <param name="prerollDurationMs">Audio kept from before speech was confirmed.</param>
    /// <param name="device">Where the model runs.</param>
    /// <param name="cancellationToken">Stops waiting for the load. The load keeps running and its result is freed.</param>
    public static async Task<VoiceActivityDetection> LoadAsync(
        int sampleRate,
        string? source = null,
        float? threshold = null,
        int? minSilenceDurationMs = null,
        int? minSpeechDurationMs = null,
        int? prerollDurationMs = null,
        InferenceDevice device = InferenceDevice.Auto,
        CancellationToken cancellationToken = default)
    {
        var load = Checked.CallAsync(() => Native.NativeMethods.LoadVoiceActivityDetection(
            source,
            Checked.ToUInt(sampleRate, nameof(sampleRate)),
            threshold,
            Checked.ToUInt(minSilenceDurationMs, nameof(minSilenceDurationMs)),
            Checked.ToUInt(minSpeechDurationMs, nameof(minSpeechDurationMs)),
            Checked.ToUInt(prerollDurationMs, nameof(prerollDurationMs)),
            SpeechNames.Of(device)));
        return new VoiceActivityDetection(await Cancellation.WaitOrDispose(load, cancellationToken).ConfigureAwait(false));
    }

    /// <summary>
    /// Feed the newest chunk of 16-bit PCM audio (not the whole recording so far) and get the
    /// confirmed state: <see cref="VoiceActivityDetectionEvent.Speech"/> or
    /// <see cref="VoiceActivityDetectionEvent.Silence"/> if unchanged since the last call, or
    /// <see cref="VoiceActivityDetectionEvent.SpeechStarted"/> or
    /// <see cref="VoiceActivityDetectionEvent.SpeechEnded"/> on the call that confirmed a change.
    /// </summary>
    public VoiceActivityDetectionEvent Push(short[] chunk)
    {
        ArgumentNullException.ThrowIfNull(chunk);
        return Checked.Call(() => _inner.Push(chunk)) switch
        {
            Native.VoiceActivityDetectionEvent.Speech => VoiceActivityDetectionEvent.Speech,
            Native.VoiceActivityDetectionEvent.SpeechStarted => VoiceActivityDetectionEvent.SpeechStarted,
            Native.VoiceActivityDetectionEvent.SpeechEnded => VoiceActivityDetectionEvent.SpeechEnded,
            Native.VoiceActivityDetectionEvent.Silence => VoiceActivityDetectionEvent.Silence,
            var other => throw new InvalidOperationException($"Unknown voice activity event {other}"),
        };
    }

    /// <summary>
    /// The current turn's audio, from the confirmed start of speech (with pre-roll) to its end, and
    /// reset for the next turn. Empty if speech was never confirmed.
    /// </summary>
    public short[] Finish() => _inner.Finish();

    /// <summary>
    /// Every speech segment in a complete recording, in order, each with a short pre-roll. Unlike
    /// <see cref="Push"/>, this finds every segment whatever the buffer size, so use it for offline
    /// processing.
    /// </summary>
    public short[][] Segment(short[] samples)
    {
        ArgumentNullException.ThrowIfNull(samples);
        return Checked.Call(() => _inner.Segment(samples));
    }

    /// <summary>Free the underlying native resources.</summary>
    public void Dispose() => _inner.Dispose();
}
