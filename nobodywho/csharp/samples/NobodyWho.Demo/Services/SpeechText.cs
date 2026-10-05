using System.Text;
using System.Text.RegularExpressions;

namespace NobodyWho.Demo.Services;

/// <summary>
/// Splits text into pieces short enough to synthesize in one call. Kokoro takes at most 509
/// phonemes at a time, and the library does not split longer text yet.
/// </summary>
public static partial class SpeechText
{
    /// <summary>
    /// Split <paramref name="text"/> into pieces of at most <paramref name="maxLength"/> characters,
    /// breaking between sentences where possible, then between clauses, then between words.
    /// </summary>
    public static List<string> Split(string text, int maxLength)
    {
        var pieces = new List<string>();
        var current = new StringBuilder();
        foreach (var sentence in Sentences().Split(text.Trim()).Where(s => s.Length > 0))
        {
            foreach (var part in Fit(sentence, maxLength))
            {
                if (current.Length > 0 && current.Length + 1 + part.Length > maxLength)
                {
                    pieces.Add(current.ToString());
                    current.Clear();
                }
                if (current.Length > 0)
                    current.Append(' ');
                current.Append(part);
            }
        }
        if (current.Length > 0)
            pieces.Add(current.ToString());
        return pieces;
    }

    /// <summary>One sentence, broken into clauses and then words until each part fits.</summary>
    private static IEnumerable<string> Fit(string sentence, int maxLength)
    {
        if (sentence.Length <= maxLength)
            return [sentence];

        var clauses = Clauses().Split(sentence).Where(c => c.Length > 0).ToArray();
        if (clauses.Length > 1)
            return Pack(clauses, maxLength);

        return Pack(sentence.Split(' ', StringSplitOptions.RemoveEmptyEntries), maxLength);
    }

    private static IEnumerable<string> Pack(IEnumerable<string> parts, int maxLength)
    {
        var current = new StringBuilder();
        foreach (var part in parts.SelectMany(p => p.Length <= maxLength ? [p] : Fit(p, maxLength)))
        {
            if (current.Length > 0 && current.Length + 1 + part.Length > maxLength)
            {
                yield return current.ToString();
                current.Clear();
            }
            if (current.Length > 0)
                current.Append(' ');
            current.Append(part);
        }
        if (current.Length > 0)
            yield return current.ToString();
    }

    // After sentence-ending punctuation (and any closing quotes or brackets), or at blank lines.
    [GeneratedRegex(@"(?<=[.!?…][""'”’)\]]*)\s+|\n\s*\n")]
    private static partial Regex Sentences();

    // After a comma, semicolon, colon or dash that is followed by a space.
    [GeneratedRegex(@"(?<=[,;:—–])\s+")]
    private static partial Regex Clauses();
}
