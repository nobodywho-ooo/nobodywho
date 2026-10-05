namespace NobodyWho.Demo.Services;

/// <summary>The models the demo uses, from the "NobodyWho" section of appsettings.json.</summary>
public sealed class DemoOptions
{
    public bool UseGpu { get; set; } = true;
    public string ChatModel { get; set; } = "";
    public string EmbeddingModel { get; set; } = "";
    public string RerankerModel { get; set; } = "";
    public string VisionModel { get; set; } = "";
    public string VisionProjection { get; set; } = "";
    public string TextToSpeech { get; set; } = "";
    public string TextToSpeechVoice { get; set; } = "";
    public string SpeechToText { get; set; } = "";
    public string VoiceActivityDetection { get; set; } = "";
}
