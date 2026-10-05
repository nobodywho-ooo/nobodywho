using System.Diagnostics;

namespace NobodyWho.Tests;

/// <summary>
/// Tests against real models. Each one skips unless its model's environment variable is set:
/// <c>TEST_MODEL</c> (a chat model such as Qwen3-0.6B), <c>TEST_EMBEDDINGS_MODEL</c> and
/// <c>TEST_CROSSENCODER_MODEL</c>.
/// </summary>
public class IntegrationTests
{
    private static readonly Dictionary<string, bool> NoThinking = new() { ["enable_thinking"] = false };

    private static CancellationToken Ct => TestContext.Current.CancellationToken;

    private static string RequireEnv(string name)
    {
        var value = Environment.GetEnvironmentVariable(name);
        Assert.SkipWhen(string.IsNullOrEmpty(value), $"{name} is not set");
        return value!;
    }

    private static Task<Model> LoadTestModel() => Model.LoadAsync(RequireEnv("TEST_MODEL"), cancellationToken: Ct);

    [Fact]
    public async Task ModelReportsItsSource()
    {
        var path = RequireEnv("TEST_MODEL");
        using var model = await Model.LoadAsync(path, cancellationToken: Ct);
        Assert.Equal(path, model.Source);
        Assert.True(model.MaxContextSize > 0);
    }

    [Fact]
    public async Task MissingModelThrows()
    {
        RequireEnv("TEST_MODEL");
        await Assert.ThrowsAsync<NobodyWhoException>(() => Model.LoadAsync("/does/not/exist.gguf", cancellationToken: Ct));
    }

    [Fact]
    public async Task Chat()
    {
        using var model = await LoadTestModel();
        using var chat = new Chat(model, systemPrompt: "Reply with one word only.", templateVariables: NoThinking);

        // Completion
        var response = await chat.Ask("Say hello").CompletedAsync(Ct);
        Assert.NotEmpty(response);

        // Streaming
        await chat.ResetContextAsync(systemPrompt: "Reply briefly.");
        var tokens = new List<string>();
        await foreach (var token in chat.Ask("Say hi").WithCancellation(Ct))
            tokens.Add(token);
        Assert.NotEmpty(tokens);

        // Tool calling
        var ping = new Tool("ping", "Ping the server", () => "pong");
        await chat.ResetContextAsync(systemPrompt: "Use the ping tool now.", tools: [ping]);
        await chat.Ask("Ping the server").CompletedAsync(Ct);
        var history = await chat.GetChatHistoryAsync();
        var toolResponse = history.OfType<Message.Tool>().FirstOrDefault();
        Assert.NotNull(toolResponse);
        Assert.Equal("pong", toolResponse.Content.PlainText);
    }

    [Fact]
    public async Task Complete()
    {
        using var model = await LoadTestModel();
        using var chat = new Chat(model, systemPrompt: "Reply with one word only.", templateVariables: NoThinking);

        Message[] messages =
        [
            new Message.User("Who was the first person to walk on the moon?"),
            new Message.Assistant("Neil Armstrong."),
            new Message.User("Which year did he do it? Answer with only the year."),
        ];
        var response = await chat.Complete(messages).CompletedAsync(Ct);
        Assert.Contains("1969", response);

        // The supplied messages replace the history, with the reply appended
        var history = await chat.GetChatHistoryAsync();
        Assert.Equal(messages.Length + 1, history.Count);
        Assert.IsType<Message.Assistant>(history[^1]);

        // An invalid conversation is rejected at the call site
        Assert.Throws<NobodyWhoException>(() =>
            chat.Complete([new Message.User("Hi"), new Message.Assistant("Aye, ")]));

        // Options stick: what they set stays set, what they omit is kept
        var thinking = new Dictionary<string, bool> { ["enable_thinking"] = true };
        await chat.Complete([new Message.User("Say hi.")], new CompletionOptions(TemplateVariables: thinking))
            .CompletedAsync(Ct);
        Assert.Equal(thinking, await chat.GetTemplateVariablesAsync());

        await chat.Complete([new Message.User("Say hi again.")]).CompletedAsync(Ct);
        Assert.Equal(thinking, await chat.GetTemplateVariablesAsync());
    }

    [Fact]
    public async Task History()
    {
        using var model = await LoadTestModel();
        using var chat = new Chat(model, systemPrompt: "Be brief.", templateVariables: NoThinking);

        Assert.Equal("Be brief.", await chat.GetSystemPromptAsync());
        await chat.SetSystemPromptAsync("Be very brief.");
        Assert.Equal("Be very brief.", await chat.GetSystemPromptAsync());

        Message[] history = [new Message.User("Hi"), new Message.Assistant("Hello!")];
        await chat.SetChatHistoryAsync(history);
        var stored = await chat.GetChatHistoryAsync();
        Assert.Equal(history, stored.Where(m => m is not Message.System));

        await chat.ResetHistoryAsync();
        Assert.DoesNotContain(await chat.GetChatHistoryAsync(), m => m is not Message.System);
    }

    [Fact]
    public async Task Tokenize()
    {
        using var model = await LoadTestModel();
        using var chat = new Chat(model, templateVariables: NoThinking);
        Assert.Equal([18665, 0], await chat.TokenizeAsync("Hey!"));
    }

    [Fact]
    public async Task Stats()
    {
        using var model = await LoadTestModel();
        using var chat = new Chat(model, templateVariables: NoThinking);
        await chat.Ask("What is the capital of Denmark?").CompletedAsync(Ct);
        var stats = await chat.GetStatsAsync();
        Assert.True(stats.ContextUsed > 0);
        Assert.True(stats.ContextUsed <= stats.ContextSize);
    }

    [Fact]
    public async Task SamplerSettings()
    {
        using var model = await LoadTestModel();
        using var chat = new Chat(model, sampler: SamplerPresets.Greedy(), templateVariables: NoThinking);
        Assert.Contains("Greedy", await chat.GetSamplerConfigJsonAsync(), StringComparison.OrdinalIgnoreCase);

        await chat.SetSamplerConfigAsync(SamplerPresets.ConstrainWithRegex("(yes|no)"));
        var answer = await chat.Ask("Is the sky blue? Answer yes or no.").CompletedAsync(Ct);
        Assert.Matches("^(yes|no)$", answer);
    }

    [Fact]
    public async Task ContextShiftOptions()
    {
        using var model = await LoadTestModel();
        using (new Chat(model, contextSize: 1024,
                   contextShift: new ContextShiftOptions(KeepFirstTurns: 2, KeepLastTurns: 3, Target: new ShiftTarget.Tokens(256))))
        {
        }
        using (new Chat(model, contextShift: new ContextShiftOptions(Enabled: false)))
        {
        }

        Assert.Throws<NobodyWhoException>(() =>
            new Chat(model, contextSize: 1024, contextShift: new ContextShiftOptions(Target: new ShiftTarget.Tokens(1024))));

        using var chat = new Chat(model);
        await chat.SetContextShiftAsync(new ContextShiftOptions(Target: new ShiftTarget.Fraction(0.25f)));
        await Assert.ThrowsAsync<NobodyWhoException>(() =>
            chat.SetContextShiftAsync(new ContextShiftOptions(KeepLastTurns: 0)));
    }

    [Fact]
    public async Task CancellingAStreamStopsGeneration()
    {
        using var model = await LoadTestModel();
        using var chat = new Chat(model, templateVariables: NoThinking);

        using var cts = CancellationTokenSource.CreateLinkedTokenSource(Ct);
        var count = 0;
        await Assert.ThrowsAnyAsync<OperationCanceledException>(async () =>
        {
            await foreach (var _ in chat.Ask("Count from 1 to 500, one number per line.").WithCancellation(cts.Token))
            {
                if (++count == 5)
                    await cts.CancelAsync();
            }
        });
        Assert.Equal(5, count);

        // The partial response is kept in the history.
        var history = await chat.GetChatHistoryAsync();
        var partial = Assert.IsType<Message.Assistant>(history[^1]);
        Assert.NotEmpty(partial.Content.PlainText);

        // The chat is still usable afterwards.
        var response = await chat.Ask("Say hi.").CompletedAsync(Ct);
        Assert.NotEmpty(response);
    }

    [Fact]
    public async Task AnAlreadyCancelledTokenStopsGeneration()
    {
        using var model = await LoadTestModel();
        using var chat = new Chat(model, templateVariables: NoThinking);

        var stopwatch = Stopwatch.StartNew();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() =>
            chat.Ask("Write a very long story about a dragon.").CompletedAsync(new CancellationToken(canceled: true)));

        // The story was stopped, so the next request does not wait behind it.
        Assert.NotEmpty(await chat.Ask("Say hi.").CompletedAsync(Ct));
        Assert.True(stopwatch.Elapsed < TimeSpan.FromSeconds(60), $"Took {stopwatch.Elapsed}");
    }

    [Fact]
    public async Task BlockingOnAUiThreadDoesNotDeadlock()
    {
        using var model = await LoadTestModel();
        using var chat = new Chat(model, templateVariables: NoThinking);

        // A UI context runs posted work only when its thread is free. This one never does, so a
        // continuation that needs it never runs, like a UI thread blocked on .Result.
        var previous = SynchronizationContext.Current;
        SynchronizationContext.SetSynchronizationContext(new NeverRunsContext());
        try
        {
            var stats = chat.GetStatsAsync();
            // Blocking is the point of this test.
#pragma warning disable xUnit1031
            Assert.True(stats.Wait(TimeSpan.FromSeconds(60), Ct), "GetStatsAsync deadlocked on the captured context");
#pragma warning restore xUnit1031
        }
        finally
        {
            SynchronizationContext.SetSynchronizationContext(previous);
        }
    }

    private sealed class NeverRunsContext : SynchronizationContext
    {
        public override void Post(SendOrPostCallback d, object? state) { }

        public override void Send(SendOrPostCallback d, object? state) { }
    }

    [Fact]
    public async Task CancellingCompletedStopsGeneration()
    {
        using var model = await LoadTestModel();
        using var chat = new Chat(model, templateVariables: NoThinking);

        using var cts = new CancellationTokenSource(TimeSpan.FromMilliseconds(500));
        var stopwatch = Stopwatch.StartNew();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() =>
            chat.Ask("Write a very long story about a dragon.").CompletedAsync(cts.Token));
        Assert.True(stopwatch.Elapsed < TimeSpan.FromSeconds(30), $"Cancellation took {stopwatch.Elapsed}");
    }

    [Fact(Timeout = 120_000)]
    public async Task AsyncToolDoesNotBlockTheCaller()
    {
        var ct = TestContext.Current.CancellationToken;
        using var model = await LoadTestModel();
        var tool = new Tool("slow_ping", "Ping the server (slow)", async () =>
        {
            await Task.Delay(2000);
            return "slow async pong";
        });
        using var chat = new Chat(model, systemPrompt: "Use the slow_ping tool now.", tools: [tool], templateVariables: NoThinking);

        var chatTask = chat.Ask("Ping the server").CompletedAsync(ct);

        // The caller's thread is free while the model runs and the tool waits.
        await Task.Delay(50, ct);
        Assert.False(chatTask.IsCompleted, "Chat should still be in progress");

        await chatTask;
        var toolResponse = (await chat.GetChatHistoryAsync()).OfType<Message.Tool>().FirstOrDefault();
        Assert.NotNull(toolResponse);
        Assert.Equal("slow async pong", toolResponse.Content.PlainText);
    }

    [Fact]
    public async Task ToolArgumentsReachTheFunction()
    {
        using var model = await LoadTestModel();
        string? seenCity = null;
        var weather = new Tool("get_weather", "Get the current weather for a city", (string city) =>
        {
            seenCity = city;
            return "Sunny, 22 degrees";
        });
        using var chat = new Chat(model, systemPrompt: "Use the get_weather tool to answer.", tools: [weather], templateVariables: NoThinking);

        await chat.Ask("What is the weather in Copenhagen?").CompletedAsync(Ct);
        Assert.NotNull(seenCity);
        Assert.Contains("Copenhagen", seenCity);
    }

    [Fact]
    public async Task FromPathOwnsItsModel()
    {
        using var chat = await NobodyWho.Chat.FromPathAsync(RequireEnv("TEST_MODEL"), templateVariables: NoThinking, cancellationToken: Ct);
        Assert.NotEmpty(await chat.Ask("Say hi.").CompletedAsync(Ct));
    }

    [Fact]
    public async Task DownloadModel()
    {
        RequireEnv("TEST_MODEL");
        const string url = "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf";

        // Download once to find the cache path, then delete it to force a fresh download.
        File.Delete(await Model.DownloadAsync(url, cancellationToken: Ct));

        var reports = new List<DownloadProgress>();
        var path = await Model.DownloadAsync(url, progress: new SyncProgress<DownloadProgress>(reports.Add), cancellationToken: Ct);
        Assert.True(new FileInfo(path).Length > 0);
        Assert.NotEmpty(reports);
        Assert.All(reports, r => Assert.True(r.Downloaded <= r.Total));
        Assert.Contains(Model.GetCachedModels(), m => m.Path == path);
    }

    [Fact]
    public async Task EncoderBatch()
    {
        using var model = await Model.LoadAsync(RequireEnv("TEST_EMBEDDINGS_MODEL"), useGpu: false, cancellationToken: Ct);
        using var encoder = new Encoder(model, contextSize: 1024);
        string[] texts = ["Copenhagen is in Denmark.", "Berlin is in Germany."];

        var individual = new List<float[]>();
        foreach (var text in texts)
            individual.Add(await encoder.EncodeAsync(text));
        var batched = await encoder.EncodeBatchAsync(texts);

        Assert.Equal(texts.Length, batched.Length);
        foreach (var (expected, actual) in individual.Zip(batched))
        {
            Assert.Equal(expected.Length, actual.Length);
            Assert.All(expected.Zip(actual), p => Assert.True(Math.Abs(p.First - p.Second) < 1e-5f));
        }
        Assert.True(Encoder.CosineSimilarity(batched[0], batched[1]) < 1f);
    }

    [Fact]
    public async Task CrossEncoderRanks()
    {
        using var reranker = await CrossEncoder.FromPathAsync(RequireEnv("TEST_CROSSENCODER_MODEL"), useGpu: false, cancellationToken: Ct);
        string[] documents = ["Paris is the capital of France.", "Bananas are yellow.", "The capital of Denmark is Copenhagen."];

        var scores = await reranker.RankAsync("What is the capital of Denmark?", documents);
        Assert.Equal(documents.Length, scores.Length);

        var ranked = await reranker.RankAndSortAsync("What is the capital of Denmark?", documents);
        Assert.Equal(documents[2], ranked[0].Document);
        Assert.True(ranked[0].Score >= ranked[^1].Score);
    }

    /// <summary>An <see cref="IProgress{T}"/> that reports on the calling thread.</summary>
    private sealed class SyncProgress<T>(Action<T> report) : IProgress<T>
    {
        public void Report(T value)
        {
            lock (this)
                report(value);
        }
    }
}
