using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;

namespace NobodyWho;

/// <summary>
/// A chat session with a local model: it keeps the conversation history and generates replies.
/// </summary>
/// <example>
/// <code>
/// using var model = await Model.LoadAsync("model.gguf");
/// using var chat = new Chat(model, systemPrompt: "You are a helpful assistant.");
///
/// await foreach (var token in chat.Ask("Hello!"))
///     Console.Write(token);
/// </code>
/// </example>
public sealed class Chat : IDisposable
{
    private readonly Native.RustChat _inner;
    private readonly Model? _ownedModel;

    /// <summary>Create a chat session on a loaded model.</summary>
    /// <param name="model">The model to chat with. It can be shared with other chats.</param>
    /// <param name="systemPrompt">The system prompt, if any.</param>
    /// <param name="contextSize">The context size, in tokens.</param>
    /// <param name="templateVariables">Variables passed to the chat template, such as <c>enable_thinking</c>.</param>
    /// <param name="tools">Tools the model may call.</param>
    /// <param name="sampler">How tokens are sampled. <c>null</c> uses <see cref="SamplerPresets.Default"/>.</param>
    /// <param name="mtp">
    /// Enables MTP speculative decoding. Requires a model loaded with a draft model.
    /// </param>
    /// <param name="threadCount">
    /// CPU threads used for inference. <c>null</c> detects the physical core count (performance
    /// cores only, on Apple silicon): hyperthreads and efficiency cores make inference slower.
    /// Lower it to leave CPU headroom for the rest of the app.
    /// </param>
    /// <param name="contextShift">How old turns are forgotten when the context is full. <c>null</c> uses the defaults.</param>
    /// <exception cref="NobodyWhoException">The chat could not be created with these settings.</exception>
    public Chat(
        Model model,
        string? systemPrompt = null,
        int contextSize = 4096,
        IReadOnlyDictionary<string, bool>? templateVariables = null,
        IEnumerable<Tool>? tools = null,
        SamplerConfig? sampler = null,
        MtpConfig? mtp = null,
        int? threadCount = null,
        ContextShiftOptions? contextShift = null)
        : this(model, null, systemPrompt, contextSize, templateVariables, tools, sampler, mtp, threadCount, contextShift)
    {
    }

    private Chat(
        Model model,
        Model? ownedModel,
        string? systemPrompt,
        int contextSize,
        IReadOnlyDictionary<string, bool>? templateVariables,
        IEnumerable<Tool>? tools,
        SamplerConfig? sampler,
        MtpConfig? mtp,
        int? threadCount,
        ContextShiftOptions? contextShift)
    {
        ArgumentNullException.ThrowIfNull(model);
        _inner = Checked.Call(() => new Native.RustChat(
            model.Inner,
            systemPrompt,
            Checked.ToUInt(contextSize, nameof(contextSize)),
            templateVariables?.ToDictionary(kv => kv.Key, kv => kv.Value),
            tools?.Select(t => t.Inner).ToArray(),
            sampler?.Inner,
            mtp?.ToNative(),
            Checked.ToUInt(threadCount, nameof(threadCount)),
            contextShift?.ToNative()));
        _ownedModel = ownedModel;
    }

    /// <summary>
    /// Load a model and create a chat on it in one step. The chat owns the model and frees it
    /// when disposed.
    /// </summary>
    /// <param name="modelPath">A local path, <c>hf://</c> path, <c>https://</c> URL, or <c>auto</c>. See <see cref="Model.LoadAsync"/>.</param>
    /// <param name="useGpu">Use GPU acceleration when available.</param>
    /// <param name="projectionModelPath">Path to an <c>mmproj</c> file, for vision and audio models.</param>
    /// <param name="draftModelPath">Path to MTP draft heads.</param>
    /// <param name="systemPrompt">The system prompt, if any.</param>
    /// <param name="contextSize">The context size, in tokens.</param>
    /// <param name="templateVariables">Variables passed to the chat template.</param>
    /// <param name="tools">Tools the model may call.</param>
    /// <param name="sampler">How tokens are sampled.</param>
    /// <param name="mtp">Enables MTP speculative decoding.</param>
    /// <param name="threadCount">CPU threads used for inference.</param>
    /// <param name="contextShift">How old turns are forgotten when the context is full.</param>
    /// <param name="progress">Receives download progress, if the model has to be downloaded.</param>
    /// <param name="cancellationToken">Stops waiting for the model to load.</param>
    public static async Task<Chat> FromPathAsync(
        string modelPath,
        bool useGpu = true,
        string? projectionModelPath = null,
        string? draftModelPath = null,
        string? systemPrompt = null,
        int contextSize = 4096,
        IReadOnlyDictionary<string, bool>? templateVariables = null,
        IEnumerable<Tool>? tools = null,
        SamplerConfig? sampler = null,
        MtpConfig? mtp = null,
        int? threadCount = null,
        ContextShiftOptions? contextShift = null,
        IProgress<DownloadProgress>? progress = null,
        CancellationToken cancellationToken = default)
    {
        Checked.ToUInt(contextSize, nameof(contextSize));
        Checked.ToUInt(threadCount, nameof(threadCount));
        var model = await Model.LoadAsync(modelPath, useGpu, projectionModelPath, draftModelPath, progress, cancellationToken)
            .ConfigureAwait(false);
        try
        {
            return new Chat(model, model, systemPrompt, contextSize, templateVariables, tools, sampler, mtp, threadCount, contextShift);
        }
        catch
        {
            model.Dispose();
            throw;
        }
    }

    /// <summary>Send a message and stream the response.</summary>
    public TokenStream Ask(string message)
    {
        ArgumentNullException.ThrowIfNull(message);
        return Stream(_inner.Ask(message));
    }

    /// <summary>Send a multimodal or JSON prompt and stream the response.</summary>
    /// <exception cref="NobodyWhoException">The prompt's JSON is invalid.</exception>
    public TokenStream Ask(Prompt prompt)
    {
        ArgumentNullException.ThrowIfNull(prompt);
        return Stream(prompt.Json is { } json
            ? Checked.Call(() => _inner.AskWithJsonPrompt(json))
            : _inner.AskWithPrompt(prompt.NativeParts()));
    }

    /// <summary>
    /// Answer a full list of messages, replacing the chat history, and stream the response.
    /// </summary>
    /// <remarks>
    /// The list is the whole conversation, used as given: it must be non-empty and end in a user
    /// or tool message. A leading system message sets the chat's system prompt; leave it out and
    /// the prompt already on the chat is kept. A later one stays in the history, for the chat
    /// template to render in place. The response is appended, and the next <see cref="Ask(string)"/>
    /// continues from there. <paramref name="options"/> follows the same rule for the chat's other
    /// settings.
    /// </remarks>
    /// <exception cref="NobodyWhoException">The conversation is not valid.</exception>
    public TokenStream Complete(IEnumerable<Message> messages, CompletionOptions? options = null)
    {
        ArgumentNullException.ThrowIfNull(messages);
        var nativeMessages = messages.Select(m => m.ToNative()).ToArray();
        var nativeOptions = (options ?? new CompletionOptions()).ToNative();
        return Stream(Checked.Call(() => _inner.Complete(nativeMessages, nativeOptions)));
    }

    /// <summary>Stop the response being generated, if any.</summary>
    public void StopGeneration() => _inner.StopGeneration();

    /// <summary>Clear the history and replace the system prompt and tools.</summary>
    public Task ResetContextAsync(string? systemPrompt = null, IEnumerable<Tool>? tools = null) =>
        Checked.CallAsync(() => _inner.ResetContext(systemPrompt, tools?.Select(t => t.Inner).ToArray()));

    /// <summary>Clear the history, keeping the system prompt and tools.</summary>
    public Task ResetHistoryAsync() => Checked.CallAsync(_inner.ResetHistory);

    /// <summary>The conversation so far.</summary>
    public async Task<IReadOnlyList<Message>> GetChatHistoryAsync()
    {
        var history = await Checked.CallAsync(_inner.GetChatHistory).ConfigureAwait(false);
        return history.Select(Message.FromNative).ToArray();
    }

    /// <summary>Replace the conversation history.</summary>
    public Task SetChatHistoryAsync(IEnumerable<Message> messages)
    {
        ArgumentNullException.ThrowIfNull(messages);
        var nativeMessages = messages.Select(m => m.ToNative()).ToArray();
        return Checked.CallAsync(() => _inner.SetChatHistory(nativeMessages));
    }

    /// <summary>The current system prompt, if any.</summary>
    public Task<string?> GetSystemPromptAsync() => Checked.CallAsync(_inner.GetSystemPrompt);

    /// <summary>Replace the system prompt. <c>null</c> removes it.</summary>
    public Task SetSystemPromptAsync(string? systemPrompt) =>
        Checked.CallAsync(() => _inner.SetSystemPrompt(systemPrompt));

    /// <summary>Change how old turns are forgotten when the context is full.</summary>
    public Task SetContextShiftAsync(ContextShiftOptions options)
    {
        ArgumentNullException.ThrowIfNull(options);
        var nativeOptions = options.ToNative();
        return Checked.CallAsync(() => _inner.SetContextShift(nativeOptions));
    }

    /// <summary>Replace the tools the model may call.</summary>
    public Task SetToolsAsync(IEnumerable<Tool> tools)
    {
        ArgumentNullException.ThrowIfNull(tools);
        var nativeTools = tools.Select(t => t.Inner).ToArray();
        return Checked.CallAsync(() => _inner.SetTools(nativeTools));
    }

    /// <summary>Set a variable passed to the chat template, such as <c>enable_thinking</c>.</summary>
    public Task SetTemplateVariableAsync(string name, bool value) =>
        Checked.CallAsync(() => _inner.SetTemplateVariable(name, value));

    /// <summary>The variables passed to the chat template.</summary>
    public async Task<IReadOnlyDictionary<string, bool>> GetTemplateVariablesAsync() =>
        await Checked.CallAsync(_inner.GetTemplateVariables).ConfigureAwait(false);

    /// <summary>Replace the sampler.</summary>
    public Task SetSamplerConfigAsync(SamplerConfig sampler)
    {
        ArgumentNullException.ThrowIfNull(sampler);
        return Checked.CallAsync(() => _inner.SetSamplerConfig(sampler.Inner));
    }

    /// <summary>The current sampler, as JSON.</summary>
    public Task<string> GetSamplerConfigJsonAsync() => Checked.CallAsync(_inner.GetSamplerConfigJson);

    /// <summary>Context usage.</summary>
    public async Task<ChatStats> GetStatsAsync()
    {
        var stats = await Checked.CallAsync(_inner.GetStats).ConfigureAwait(false);
        return new ChatStats(checked((int)stats.ContextSize), checked((int)stats.ContextUsed));
    }

    /// <summary>
    /// The MTP draft acceptance rate for the most recent response, in [0, 1]. <c>null</c> when MTP
    /// is off or no drafts were proposed.
    /// </summary>
    public Task<float?> GetMtpAcceptanceRateAsync() => Checked.CallAsync(_inner.MtpAcceptanceRate);

    /// <summary>Tokenize text with this chat's model.</summary>
    public async Task<IReadOnlyList<int>> TokenizeAsync(string text)
    {
        var tokens = await Checked.CallAsync(() => _inner.Tokenize(text)).ConfigureAwait(false);
        return tokens.Select(t => t ?? throw new InvalidOperationException("Plain text produced a media token.")).ToArray();
    }

    /// <summary>
    /// Tokenize a multimodal prompt. Text produces token IDs; each image or audio embedding slot
    /// produces <c>null</c>.
    /// </summary>
    public async Task<IReadOnlyList<int?>> TokenizeAsync(Prompt prompt)
    {
        ArgumentNullException.ThrowIfNull(prompt);
        var parts = prompt.NativeParts();
        return await Checked.CallAsync(() => _inner.TokenizeWithPrompt(parts)).ConfigureAwait(false);
    }

    /// <summary>
    /// Free the underlying native resources, and the model too if this chat was created with
    /// <see cref="FromPathAsync"/>.
    /// </summary>
    public void Dispose()
    {
        _inner.Dispose();
        _ownedModel?.Dispose();
    }

    private TokenStream Stream(Native.RustTokenStream stream) => new(stream, _inner.StopGeneration);
}
