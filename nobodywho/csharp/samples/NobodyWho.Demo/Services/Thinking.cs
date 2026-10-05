namespace NobodyWho.Demo.Services;

/// <summary>
/// Separates a reasoning model's <c>&lt;think&gt;…&lt;/think&gt;</c> block from its answer, so the
/// page can show the reasoning quietly while it streams.
/// </summary>
public readonly record struct Thinking(string? Thought, string Answer, bool StillThinking)
{
    private const string Open = "<think>";
    private const string Close = "</think>";

    public static Thinking Split(string text)
    {
        var start = text.IndexOf(Open, StringComparison.Ordinal);
        if (start < 0)
            return new(null, text, false);

        var end = text.IndexOf(Close, start, StringComparison.Ordinal);
        if (end < 0)
            return new(text[(start + Open.Length)..].Trim(), text[..start].Trim(), true);

        var thought = text[(start + Open.Length)..end].Trim();
        var answer = (text[..start] + text[(end + Close.Length)..]).Trim();
        return new(thought.Length == 0 ? null : thought, answer, false);
    }
}
