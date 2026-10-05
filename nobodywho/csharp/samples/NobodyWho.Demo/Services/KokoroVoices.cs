namespace NobodyWho.Demo.Services;

/// <summary>
/// The Kokoro voices NobodyWho can speak with, from the NobodyWho/Kokoro-82M repo.
/// </summary>
/// <remarks>
/// A voice's name says its language and gender: <c>bf_emma</c> is a British English female voice.
/// The repo also has Hindi, Japanese and Chinese voices, which are left out because NobodyWho's
/// phonemizer only supports the languages below.
/// </remarks>
public static class KokoroVoices
{
    public sealed record Voice(string Id, string Name, string Gender, Language Language);

    public sealed record Language(char Prefix, string Code, string Name, string Sample);

    public static readonly IReadOnlyList<Language> Languages =
    [
        new('a', "en-us", "American English", "Hello from NobodyWho. This voice was made on this computer, without sending a single word to the cloud."),
        new('b', "en-gb", "British English", "Hello from NobodyWho. This voice was made on this computer, without sending a single word to the cloud."),
        new('e', "es", "Spanish", "Hola desde NobodyWho. Esta voz se creó en este ordenador, sin enviar ni una sola palabra a la nube."),
        new('f', "fr", "French", "Bonjour de la part de NobodyWho. Cette voix a été créée sur cet ordinateur, sans envoyer un seul mot dans le cloud."),
        new('i', "it", "Italian", "Ciao da NobodyWho. Questa voce è stata creata su questo computer, senza inviare una sola parola al cloud."),
        new('p', "pt-br", "Brazilian Portuguese", "Olá do NobodyWho. Esta voz foi criada neste computador, sem enviar uma única palavra para a nuvem."),
    ];

    private static readonly string[] Ids =
    [
        "af_alloy", "af_aoede", "af_bella", "af_heart", "af_jessica", "af_kore", "af_nicole", "af_nova", "af_river", "af_sarah", "af_sky",
        "am_adam", "am_echo", "am_eric", "am_fenrir", "am_liam", "am_michael", "am_onyx", "am_puck", "am_santa",
        "bf_alice", "bf_emma", "bf_isabella", "bf_lily",
        "bm_daniel", "bm_fable", "bm_george", "bm_lewis",
        "ef_dora", "em_alex", "em_santa",
        "ff_siwis",
        "if_sara", "im_nicola",
        "pf_dora", "pm_alex", "pm_santa",
    ];

    public static readonly IReadOnlyList<Voice> All = Ids.Select(Describe).OfType<Voice>().ToArray();

    /// <summary>The voice with this id, or <c>null</c> if it's not a supported Kokoro voice.</summary>
    public static Voice? Find(string? id) => All.FirstOrDefault(v => v.Id == id) ?? Describe(id);

    /// <summary>Whether a text-to-speech source is a Kokoro model, whose voices are listed here.</summary>
    public static bool IsKokoro(string source) => source.Contains("kokoro", StringComparison.OrdinalIgnoreCase);

    private static Voice? Describe(string? id)
    {
        if (id is not { Length: > 3 } || id[2] != '_')
            return null;
        var language = Languages.FirstOrDefault(l => l.Prefix == id[0]);
        if (language is null)
            return null;
        var gender = id[1] switch { 'f' => "female", 'm' => "male", _ => "" };
        var name = char.ToUpperInvariant(id[3]) + id[4..];
        return new Voice(id, name, gender, language);
    }
}
