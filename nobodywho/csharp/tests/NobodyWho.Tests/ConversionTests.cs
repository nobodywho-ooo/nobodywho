using System.Text.Json.Nodes;

namespace NobodyWho.Tests;

public class ConversionTests
{
    [Fact]
    public void MessagesRoundTripThroughTheNativeLayer()
    {
        Message[] messages =
        [
            new Message.System("Be brief."),
            new Message.User(new MessageContent.Parts(
                new ContentPart.Text("What is this?"),
                new ContentPart.Image("photo.png"),
                new ContentPart.Audio("clip.wav"))),
            new Message.Assistant("Calling a tool.", [new ToolCall("look", """{"at": "photo"}""")]),
            new Message.Tool("look", "A cat."),
            new Message.User(new MessageContent.Json("""{"q": 1}""")),
        ];

        var roundTripped = messages.Select(m => Message.FromNative(m.ToNative())).ToArray();

        Assert.Equal(messages.Length, roundTripped.Length);
        Assert.Equal(messages[0], roundTripped[0]);
        Assert.Equal(messages[3], roundTripped[3]);
        Assert.Equal(messages[4], roundTripped[4]);

        var user = Assert.IsType<Message.User>(roundTripped[1]);
        var parts = Assert.IsType<MessageContent.Parts>(user.Content);
        Assert.Equal(
            [new ContentPart.Text("What is this?"), new ContentPart.Image("photo.png"), new ContentPart.Audio("clip.wav")],
            parts.Items);

        var assistant = Assert.IsType<Message.Assistant>(roundTripped[2]);
        Assert.Equal("Calling a tool.", assistant.Content.PlainText);
        Assert.Equal(new ToolCall("look", """{"at": "photo"}"""), Assert.Single(assistant.ToolCalls!));
    }

    [Fact]
    public void MessagesRoundTripThroughJson()
    {
        Message[] messages =
        [
            new Message.System("Be brief."),
            new Message.User(new MessageContent.Parts(new ContentPart.Text("What is this?"), new ContentPart.Image("photo.png"))),
            new Message.Assistant("Calling a tool.", [new ToolCall("look", """{"at": "photo"}""")]),
            new Message.Tool("look", new MessageContent.Json("""{"animal": "cat"}""")),
        ];

        var json = System.Text.Json.JsonSerializer.Serialize(messages);
        var restored = System.Text.Json.JsonSerializer.Deserialize<Message[]>(json)!;

        Assert.Contains("\"role\":\"user\"", json);
        Assert.Equal(messages.Length, restored.Length);
        Assert.Equal(messages[0], restored[0]);
        var parts = Assert.IsType<MessageContent.Parts>(Assert.IsType<Message.User>(restored[1]).Content);
        Assert.Equal([new ContentPart.Text("What is this?"), new ContentPart.Image("photo.png")], parts.Items);
        var assistant = Assert.IsType<Message.Assistant>(restored[2]);
        Assert.Equal("Calling a tool.", assistant.Content.PlainText);
        Assert.Equal(new ToolCall("look", """{"at": "photo"}"""), Assert.Single(assistant.ToolCalls!));
        Assert.Equal(messages[3], restored[3]);
    }

    [Fact]
    public void PlainTextLeavesOutMedia()
    {
        MessageContent content = new MessageContent.Parts(
            new ContentPart.Text("Look at "),
            new ContentPart.Image("a.png"),
            new ContentPart.Text("this."));
        Assert.Equal("Look at this.", content.PlainText);
        Assert.Equal("hi", ((MessageContent)"hi").PlainText);
    }

    [Fact]
    public void ContextShiftOptionsConvert()
    {
        var native = new ContextShiftOptions(KeepFirstTurns: 2, KeepLastTurns: 3, Target: new ShiftTarget.Tokens(256)).ToNative();
        Assert.True(native.Enabled);
        Assert.Equal(2u, native.KeepFirstTurns);
        Assert.Equal(3u, native.KeepLastTurns);
        Assert.Equal(256u, Assert.IsType<Native.ShiftTarget.Tokens>(native.Target).TokensValue);

        Assert.Throws<ArgumentOutOfRangeException>(() => new ContextShiftOptions(KeepLastTurns: -1).ToNative());
    }

    [Fact]
    public void JsonPromptsSerializeValues()
    {
        var prompt = Prompt.FromJson(new { question = "2 + 2?", options = new[] { 3, 4 } });
        Assert.True(JsonNode.DeepEquals(
            JsonNode.Parse("""{"question": "2 + 2?", "options": [3, 4]}"""),
            JsonNode.Parse(prompt.Json!)));
        Assert.Empty(prompt.Parts);
        Assert.Throws<InvalidOperationException>(() => prompt.NativeParts());
    }

    [Fact]
    public void SamplerBuilderProducesConfig()
    {
        var builder = new SamplerBuilder().TopK(40).Temperature(0.5f);
        var json = JsonNode.Parse(builder.Seed(7).Dist().ToJson())!.ToJsonString();
        Assert.Contains("40", json);
        Assert.Contains("0.5", json);

        // Builders are immutable: the base can be reused.
        Assert.NotEqual(builder.Greedy().ToJson(), builder.Dist().ToJson());
    }

    [Fact]
    public void SamplerConfigRoundTripsThroughJson()
    {
        var config = SamplerPresets.Temperature(0.3f);
        var copy = SamplerConfig.FromJson(config.ToJson());
        Assert.Equal(config.ToJson(), copy.ToJson());

        Assert.Throws<NobodyWhoException>(() => SamplerConfig.FromJson("not json"));
    }

    [Fact]
    public void AllPresetsAreAvailable()
    {
        SamplerConfig[] presets =
        [
            SamplerPresets.Default(),
            SamplerPresets.TopK(20),
            SamplerPresets.TopP(0.9f),
            SamplerPresets.Greedy(),
            SamplerPresets.Temperature(0.7f),
            SamplerPresets.Dry(),
            SamplerPresets.Json(),
            SamplerPresets.ConstrainWithJsonSchema("""{"type": "object"}"""),
            SamplerPresets.ConstrainWithRegex("[a-z]+"),
            SamplerPresets.ConstrainWithGrammar("start: \"yes\" | \"no\""),
        ];
        Assert.All(presets, p => Assert.NotEmpty(p.ToJson()));
    }

    [Fact]
    public void CosineSimilarityOfVectors()
    {
        Assert.Equal(1f, Encoder.CosineSimilarity([1f, 0f], [2f, 0f]), 5);
        Assert.Equal(0f, Encoder.CosineSimilarity([1f, 0f], [0f, 3f]), 5);
    }
}
