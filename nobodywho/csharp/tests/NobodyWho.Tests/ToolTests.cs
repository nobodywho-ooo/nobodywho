using System.ComponentModel;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace NobodyWho.Tests;

public enum Unit
{
    Celsius,
    Fahrenheit,
}

public class ToolTests
{
    private static JsonNode Schema(Tool tool) => JsonNode.Parse(tool.GetSchemaJson())!;

    [Fact]
    public void SchemaDescribesParameters()
    {
        var tool = new Tool(
            "get_weather",
            "Get the weather",
            ([Description("The city name")] string city, Unit unit, int days, double threshold, bool verbose) => city);

        var schema = Schema(tool);
        Assert.Equal("object", (string?)schema["type"]);
        Assert.Equal(["city", "unit", "days", "threshold", "verbose"],
            schema["required"]!.AsArray().Select(n => (string?)n));

        var properties = schema["properties"]!;
        Assert.Equal("string", (string?)properties["city"]!["type"]);
        Assert.Equal("The city name", (string?)properties["city"]!["description"]);
        Assert.Equal(["Celsius", "Fahrenheit"], properties["unit"]!["enum"]!.AsArray().Select(n => (string?)n));
        Assert.Equal("integer", (string?)properties["days"]!["type"]);
        Assert.Equal("number", (string?)properties["threshold"]!["type"]);
        Assert.Equal("boolean", (string?)properties["verbose"]!["type"]);
    }

    [Fact]
    public void SchemaDescribesCollections()
    {
        var tool = new Tool(
            "collect",
            "Collections",
            (string[] tags, List<int> counts, Dictionary<string, double> scores, int? maybe) => "");

        var properties = Schema(tool)["properties"]!;
        Assert.Equal("array", (string?)properties["tags"]!["type"]);
        Assert.Equal("string", (string?)properties["tags"]!["items"]!["type"]);
        Assert.Equal("integer", (string?)properties["counts"]!["items"]!["type"]);
        Assert.Equal("object", (string?)properties["scores"]!["type"]);
        Assert.Equal("number", (string?)properties["scores"]!["additionalProperties"]!["type"]);
        Assert.Equal("integer", (string?)properties["maybe"]!["type"]);
    }

    [Fact]
    public void UnsupportedParameterTypeThrows()
    {
        var e = Assert.Throws<ArgumentException>(() => new Tool("bad", "Bad", (Uri url) => url.ToString()));
        Assert.Contains("Unsupported tool parameter type", e.Message);
    }

    [Fact]
    public void NonStringKeyedDictionaryThrows() =>
        Assert.Throws<ArgumentException>(() => new Tool("bad", "Bad", (Dictionary<int, string> map) => ""));

    [Fact]
    public void NonStringReturnTypeThrows() =>
        Assert.Throws<ArgumentException>(() => new Tool("bad", "Bad", (int x) => x));

    [Fact]
    public void ArgumentsAreConverted()
    {
        var tool = new Tool(
            "echo",
            "Echo",
            (string city, Unit unit, int days, List<string> tags, Dictionary<string, int> counts, bool? flag) =>
                $"{city}|{unit}|{days}|{string.Join(",", tags)}|{counts["a"]}|{flag?.ToString() ?? "null"}");

        var result = tool.Handler("""
            {"city": "Oslo", "unit": "Fahrenheit", "days": 3, "tags": ["x", "y"], "counts": {"a": 7}}
            """);
        Assert.Equal("Oslo|Fahrenheit|3|x,y|7|null", result);
    }

    [Fact]
    public void AsyncFunctionsAreAwaited()
    {
        var tool = new Tool("ping", "Ping", async () =>
        {
            await Task.Delay(10);
            return "async pong";
        });
        Assert.Equal("async pong", tool.Handler("{}"));

        var valueTaskTool = new Tool("ping", "Ping", () => ValueTask.FromResult("value pong"));
        Assert.Equal("value pong", valueTaskTool.Handler("{}"));
    }

    [Fact]
    public void ExceptionsAreReportedToTheModel()
    {
        var tool = new Tool("fail", "Fail", string (string reason) => throw new InvalidOperationException(reason));
        Assert.Equal("Error: no luck", tool.Handler("""{"reason": "no luck"}"""));

        var missing = new Tool("needs", "Needs an argument", (int count) => count.ToString());
        Assert.StartsWith("Error: ", missing.Handler("{}"));
    }

    [Fact]
    public void NullResultsBecomeEmptyStrings()
    {
        var tool = new Tool("lookup", "Lookup", string? (string key) => null);
        Assert.Equal("", tool.Handler("""{"key": "x"}"""));

        var raw = new Tool("raw", "Raw", [], _ => null!);
        Assert.Equal("", raw.Handler("{}"));
    }

    [Fact]
    public void InvalidRawSchemaThrows() =>
        Assert.Throws<ArgumentException>(() =>
            new Tool("raw", "Raw", [new ToolParameter("n", "{not json")], _ => ""));

    [Fact]
    public void DefaultValuesFillMissingArguments()
    {
        string Greet(string name, string greeting = "Hello") => $"{greeting}, {name}";
        var tool = new Tool("greet", "Greet", Greet);
        Assert.Equal("Hello, Ada", tool.Handler("""{"name": "Ada"}"""));
    }

    [Fact]
    public void RawToolReceivesArguments()
    {
        var tool = new Tool(
            "raw",
            "Raw",
            [new ToolParameter("n", """{"type": "integer"}""")],
            args => (args.GetProperty("n").GetInt32() * 2).ToString());

        Assert.Equal("42", tool.Handler("""{"n": 21}"""));
        var schema = Schema(tool);
        Assert.Equal("integer", (string?)schema["properties"]!["n"]!["type"]);
    }

    [Fact]
    public void ToolCallParsesArguments()
    {
        var call = new ToolCall("f", """{"x": 1}""");
        Assert.Equal(1, call.Arguments.GetProperty("x").GetInt32());
        Assert.Equal(JsonValueKind.Object, call.Arguments.ValueKind);
    }
}
