namespace NobodyWho.Enums;

/// <summary>
/// The enum that represents the valid sampler methods for the <see cref="NobodyWhoSampler"/>.
/// </summary>
public enum SamplerMethod
{
    /// <summary>
    /// The <strong>DRY</strong> (<strong>D</strong>on't <strong>R</strong>epeat <strong>Y</strong>ourself) sampler method,
    /// an effective technique for reducing repetition in generated text even across long contexts by penalizing tokens based on their recent usage patterns.
    /// </summary>
    /// <remarks>
    /// See: <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/main/README.md#dry-repetition-penalty"/>
    /// </remarks>
    DRY,

    /// <summary>
    /// The <strong>Greedy</strong> sampler method always selects the token with the highest probability.
    /// This is the simplest sampling method and produces deterministic output.
    /// </summary>
    /// <remarks>
    /// See: <see href="https://kojix2.github.io/llama.cr/Llama/Sampler/Greedy.html"/>
    /// </remarks>
    Greedy,

    /// <summary>
    /// The <strong>MinP</strong> sampler method, which was designed as an alternative to <see cref="TopP"/>, and aims to ensure a balance of quality and variety.
    /// The parameter <strong>P</strong> represents the minimum probability for a token to be considered, relative to the probability of the most likely token.
    /// </summary>
    /// <remarks>
    /// See:
    /// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/main/README.md#min-p-sampling"/>
    /// <br/>• <see href="https://kojix2.github.io/llama.cr/Llama/Sampler/MinP.html"/>
    /// </remarks>
    MinP,

    /// <summary>
    /// The <strong>MirostatV1</strong> sampler method is an algorithm that actively maintains the quality of generated text within a desired range during text generation.
    /// It aims to strike a balance between coherence and diversity, avoiding low-quality output caused by excessive repetition (boredom traps) or incoherence (confusion traps).
    /// </summary>
    /// <remarks>
    /// See:
    /// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/main/README.md#mirostat-sampling"/>
    /// <br/>• <see href="https://kojix2.github.io/llama.cr/Llama/Sampler/Mirostat.html"/>
    /// </remarks>
    MirostatV1,

    /// <summary>
    /// The <strong>MirostatV2</strong> sampler method is an improved and more efficient version of <see cref="MirostatV1"/> that dynamically adjusts sampling to maintain a target entropy level.
    /// </summary>
    /// <remarks>
    /// See:
    /// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/main/README.md#mirostat-sampling"/>
    /// <br/>• <see href="https://kojix2.github.io/llama.cr/Llama/Sampler/MirostatV2.html"/>
    /// </remarks>
    MirostatV2,

    /// <summary>
    /// The <strong>Temperature</strong> sampler method, which adjusts the logits by dividing them by the temperature value. Higher temperatures (&gt;1.0) make the distribution more uniform, leading to more random outputs.
    /// Lower temperatures (&lt;1.0) make the distribution more peaked, leading to more deterministic outputs.
    /// </summary>
    /// <remarks>
    /// See:
    /// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/main/README.md#temperature"/>
    /// <br/>• <see href="https://kojix2.github.io/llama.cr/Llama/Sampler/Temp.html"/>
    /// </remarks>
    Temperature,

    /// <summary>
    /// The <strong>TopK</strong> sampler method, which is a text generation method that selects the next token only from the top k most likely tokens predicted by the model.
    /// It helps reduce the risk of generating low-probability or nonsensical tokens, but it may also limit the diversity of the output.
    /// </summary>
    /// <remarks>
    /// See:
    /// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/main/README.md#top-k-sampling"/>
    /// <br/>• <see href="https://kojix2.github.io/llama.cr/Llama/Sampler/TopK.html"/>
    /// </remarks>
    TopK,

    /// <summary>
    /// The <strong>TopP</strong> sampler method, which is also known as nucleus sampling, is another text generation method that selects the next token from a subset of tokens that together have a cumulative probability of at least <strong>P</strong>.
    /// This method provides a balance between diversity and quality by considering both the probabilities of tokens and the number of tokens to sample from.
    /// </summary>
    /// <remarks>
    /// See:
    /// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/main/README.md#top-p-sampling"/>
    /// <br/>• <see href="https://kojix2.github.io/llama.cr/Llama/Sampler/TopP.html"/>
    /// </remarks>
    TopP,

    /// <summary>
    /// The <strong>TypicalP</strong> sampler method, which promotes the generation of contextually coherent and diverse text by sampling tokens that are typical or expected based on the surrounding context.
    /// By setting the parameter <strong>P</strong> between 0 and 1, you can control the balance between producing text that is locally coherent and diverse.
    /// </summary>
    /// <remarks>
    /// See:
    /// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/main/README.md#locally-typical-sampling"/>
    /// <br/>• <see href="https://kojix2.github.io/llama.cr/Llama/Sampler/Typical.html"/>
    /// </remarks>
    TypicalP,

    /// <summary>
    /// The <strong>XTC</strong> (E<strong>X</strong>clude <strong>T</strong>op <strong>C</strong>hoices) sampler method is a unique method that is designed to remove top tokens from consideration and avoid more obvious and repetitive outputs.
    /// </summary>
    /// <remarks>
    /// See:
    /// <br/>• <see href="https://github.com/ggml-org/llama.cpp/blob/5c0eb5ef544aeefd81c303e03208f768e158d93c/tools/main/README.md#locally-typical-sampling"/>
    /// <br/>• <see href="https://kojix2.github.io/llama.cr/Llama/Sampler/Xtc.html"/>
    /// </remarks>
    XTC
}