using Microsoft.Extensions.Options;

namespace NobodyWho.Demo.Services;

/// <summary>
/// The models every page shares. Models are large, so each one is loaded once for the whole app;
/// pages create their own cheap <see cref="Chat"/> sessions on top.
/// </summary>
public sealed class ModelHub : IDisposable
{
    public ModelHub(IOptions<DemoOptions> options)
    {
        var o = options.Value;
        Options = o;

        ChatModel = new("Chat", "Chat, tools, structured output and answers from documents", o.ChatModel,
            progress => Model.LoadAsync(o.ChatModel, o.UseGpu, progress: progress));
        Encoder = new("Embeddings", "Semantic search", o.EmbeddingModel,
            progress => NobodyWho.Encoder.FromPathAsync(o.EmbeddingModel, o.UseGpu, progress: progress));
        Reranker = new("Reranker", "Reranking search results", o.RerankerModel,
            progress => CrossEncoder.FromPathAsync(o.RerankerModel, o.UseGpu, progress: progress));
        VisionModel = new("Vision", "Questions about images", o.VisionModel,
            progress => Model.LoadAsync(o.VisionModel, o.UseGpu, projectionModelPath: NullIfEmpty(o.VisionProjection), progress: progress));
        TextToSpeech = new("Text to speech", "Reading text aloud", o.TextToSpeech,
            _ => LoadVoiceAsync(NullIfEmpty(o.TextToSpeechVoice)));
        SpeechToText = new("Speech to text", "Transcribing audio", o.SpeechToText,
            _ => NobodyWho.SpeechToText.LoadAsync(o.SpeechToText));
    }

    public DemoOptions Options { get; }
    public ModelSlot<Model> ChatModel { get; }
    public ModelSlot<Encoder> Encoder { get; }
    public ModelSlot<CrossEncoder> Reranker { get; }
    public ModelSlot<Model> VisionModel { get; }
    public ModelSlot<TextToSpeech> TextToSpeech { get; }
    public ModelSlot<SpeechToText> SpeechToText { get; }

    public IReadOnlyList<IModelSlot> All => [ChatModel, Encoder, Reranker, VisionModel, TextToSpeech, SpeechToText];

    /// <summary>
    /// Load the text-to-speech model with <paramref name="voice"/>. A voice is fixed when the model
    /// loads, so each voice needs its own instance. Kokoro voices also get their language, which
    /// their name encodes.
    /// </summary>
    public Task<TextToSpeech> LoadVoiceAsync(string? voice, CancellationToken cancellationToken = default)
    {
        var language = KokoroVoices.IsKokoro(Options.TextToSpeech) ? KokoroVoices.Find(voice)?.Language.Code : null;
        return NobodyWho.TextToSpeech.LoadAsync(Options.TextToSpeech, voice: voice, language: language, cancellationToken: cancellationToken);
    }

    private static string? NullIfEmpty(string value) => string.IsNullOrWhiteSpace(value) ? null : value;

    public void Dispose()
    {
        ChatModel.Dispose();
        Encoder.Dispose();
        Reranker.Dispose();
        VisionModel.Dispose();
        TextToSpeech.Dispose();
        SpeechToText.Dispose();
    }
}
