using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Linq;
using System.Reflection;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Json.Serialization;
using System.Threading.Tasks;

namespace NobodyWho;

/// <summary>
/// A function the model can call while it answers.
/// </summary>
/// <remarks>
/// <para>
/// Pass any delegate: a lambda, a local function or a method group. The parameter names and
/// types become the tool's JSON schema, and the model's arguments are converted back to those
/// types when it calls the tool. Describe parameters with <see cref="DescriptionAttribute"/>.
/// </para>
/// <para>
/// Supported parameter types are <see cref="string"/>, <see cref="bool"/>, the integer and
/// floating-point types, enums, nullable versions of these, arrays and lists of a supported
/// type, and dictionaries with <see cref="string"/> keys. Every parameter is required in the
/// schema the model sees, including ones with default values. The function must return
/// <see cref="string"/>, <see cref="Task{TResult}">Task&lt;string&gt;</see> or
/// <see cref="ValueTask{TResult}">ValueTask&lt;string&gt;</see>.
/// </para>
/// <para>
/// The function runs on the inference thread, which waits for it, so an async function does not
/// block your own threads. If it throws, the model is shown <c>Error: </c> followed by the
/// exception's message.
/// </para>
/// </remarks>
/// <example>
/// <code>
/// var weather = new Tool(
///     "get_weather",
///     "Get the current weather for a city",
///     ([Description("The city name")] string city, bool celsius) =>
///         $"{{\"city\": \"{city}\", \"temperature\": 22, \"celsius\": {celsius.ToString().ToLower()}}}");
///
/// using var chat = new Chat(model, tools: [weather]);
/// </code>
/// </example>
public sealed class Tool
{
    private static readonly JsonSerializerOptions ArgumentOptions = new()
    {
        Converters = { new JsonStringEnumConverter() },
        NumberHandling = JsonNumberHandling.AllowReadingFromString,
    };

    internal Native.RustTool Inner { get; }

    /// <summary>Runs the tool on the model's arguments, as the inference thread does.</summary>
    internal Func<string, string> Handler { get; }

    /// <summary>The tool's name, as the model sees it.</summary>
    public string Name { get; }

    /// <summary>The tool's description, as the model sees it.</summary>
    public string Description { get; }

    /// <summary>Create a tool from a delegate.</summary>
    /// <param name="name">The tool's name, as the model sees it.</param>
    /// <param name="description">What the tool does, so the model knows when to call it.</param>
    /// <param name="function">The function to run when the model calls the tool.</param>
    /// <exception cref="ArgumentException">
    /// The function has a parameter type that cannot be described as JSON, or does not return a string.
    /// </exception>
    public Tool(string name, string description, Delegate function)
    {
        ArgumentNullException.ThrowIfNull(name);
        ArgumentNullException.ThrowIfNull(description);
        ArgumentNullException.ThrowIfNull(function);

        var method = function.Method;
        var await = ResultAwaiter(method.ReturnType)
            ?? throw new ArgumentException(
                $"Tool function must return string, Task<string> or ValueTask<string>, but returns {method.ReturnType}.",
                nameof(function));

        var parameters = method.GetParameters();
        var toolParameters = parameters.Select(p => new Native.ToolParameter(
            p.Name ?? throw new ArgumentException("Tool function parameters must have names.", nameof(function)),
            ToolSchema.ForParameter(p).ToJsonString())).ToArray();

        Name = name;
        Description = description;
        Handler = Guarded(argumentsJson =>
        {
            using var doc = JsonDocument.Parse(argumentsJson);
            var args = parameters.Select(p => ConvertArgument(doc.RootElement, p)).ToArray();
            object? result;
            try
            {
                result = function.DynamicInvoke(args);
            }
            catch (TargetInvocationException e) when (e.InnerException is not null)
            {
                System.Runtime.ExceptionServices.ExceptionDispatchInfo.Throw(e.InnerException);
                throw;
            }
            return await(result);
        });
        Inner = new Native.RustTool(name, description, toolParameters, new Callback(Handler));
    }

    /// <summary>
    /// Create a tool from explicit JSON schemas and a callback that receives the raw arguments.
    /// Use this when the parameters cannot be described by a delegate's signature.
    /// </summary>
    /// <param name="name">The tool's name, as the model sees it.</param>
    /// <param name="description">What the tool does, so the model knows when to call it.</param>
    /// <param name="parameters">The tool's parameters, in order. All of them are required.</param>
    /// <param name="function">Receives the arguments as a JSON object and returns the tool's result.</param>
    public Tool(string name, string description, IEnumerable<ToolParameter> parameters, Func<JsonElement, string> function)
    {
        ArgumentNullException.ThrowIfNull(name);
        ArgumentNullException.ThrowIfNull(description);
        ArgumentNullException.ThrowIfNull(parameters);
        ArgumentNullException.ThrowIfNull(function);

        var parameterList = parameters.ToArray();
        foreach (var parameter in parameterList)
        {
            try
            {
                using var _ = JsonDocument.Parse(parameter.JsonSchema);
            }
            catch (JsonException e)
            {
                throw new ArgumentException(
                    $"Parameter '{parameter.Name}' has an invalid JSON schema: {e.Message}", nameof(parameters), e);
            }
        }

        Name = name;
        Description = description;
        Handler = Guarded(argumentsJson =>
        {
            using var doc = JsonDocument.Parse(argumentsJson);
            return function(doc.RootElement);
        });
        Inner = new Native.RustTool(
            name,
            description,
            parameterList.Select(p => new Native.ToolParameter(p.Name, p.JsonSchema)).ToArray(),
            new Callback(Handler));
    }

    /// <summary>The JSON schema of the tool's parameters.</summary>
    public string GetSchemaJson() => Inner.GetSchemaJson();

    private static Func<object?, string>? ResultAwaiter(Type returnType)
    {
        if (returnType == typeof(string))
            return r => (string)r!;
        if (returnType == typeof(Task<string>))
            return r => ((Task<string>)r!).GetAwaiter().GetResult();
        if (returnType == typeof(ValueTask<string>))
            return r => ((ValueTask<string>)r!).AsTask().GetAwaiter().GetResult();
        return null;
    }

    private static object? ConvertArgument(JsonElement args, ParameterInfo parameter)
    {
        if (args.ValueKind == JsonValueKind.Object && args.TryGetProperty(parameter.Name!, out var value))
            return value.Deserialize(parameter.ParameterType, ArgumentOptions);
        if (parameter.HasDefaultValue)
            return parameter.DefaultValue;
        if (!parameter.ParameterType.IsValueType || Nullable.GetUnderlyingType(parameter.ParameterType) is not null)
            return null;
        throw new ArgumentException($"Missing argument '{parameter.Name}'.");
    }

    /// <summary>
    /// Exceptions must never unwind into native code, so report them to the model instead.
    /// </summary>
    private static Func<string, string> Guarded(Func<string, string?> call) => argumentsJson =>
    {
        try
        {
            // A null string cannot cross into native code either.
            return call(argumentsJson) ?? string.Empty;
        }
        catch (Exception e)
        {
            return $"Error: {e.Message}";
        }
    };

    private sealed class Callback(Func<string, string> call) : Native.RustToolCallback
    {
        public string Call(string argumentsJson) => call(argumentsJson);
    }
}

/// <summary>A tool parameter, for <see cref="Tool(string, string, IEnumerable{ToolParameter}, Func{JsonElement, string})"/>.</summary>
/// <param name="Name">The parameter's name.</param>
/// <param name="JsonSchema">The parameter's JSON schema, such as <c>{"type": "string"}</c>.</param>
public sealed record ToolParameter(string Name, string JsonSchema);

/// <summary>Builds JSON schemas from .NET types.</summary>
internal static class ToolSchema
{
    internal static JsonObject ForParameter(ParameterInfo parameter)
    {
        var schema = ForType(parameter.ParameterType, parameter.Name);
        if (parameter.GetCustomAttribute<DescriptionAttribute>() is { Description: { Length: > 0 } description })
            schema["description"] = description;
        return schema;
    }

    internal static JsonObject ForType(Type type, string? name = null)
    {
        if (Nullable.GetUnderlyingType(type) is { } underlying)
            return ForType(underlying, name);

        if (type == typeof(string) || type == typeof(char))
            return new JsonObject { ["type"] = "string" };
        if (type == typeof(bool))
            return new JsonObject { ["type"] = "boolean" };
        if (type == typeof(int) || type == typeof(long) || type == typeof(short) || type == typeof(sbyte)
            || type == typeof(uint) || type == typeof(ulong) || type == typeof(ushort) || type == typeof(byte))
            return new JsonObject { ["type"] = "integer" };
        if (type == typeof(double) || type == typeof(float) || type == typeof(decimal))
            return new JsonObject { ["type"] = "number" };
        if (type.IsEnum)
            return new JsonObject
            {
                ["type"] = "string",
                ["enum"] = new JsonArray(Enum.GetNames(type).Select(n => (JsonNode)n).ToArray()),
            };

        if (DictionaryValueType(type) is { } valueType)
            return new JsonObject { ["type"] = "object", ["additionalProperties"] = ForType(valueType, name) };
        if (ElementType(type) is { } elementType)
            return new JsonObject { ["type"] = "array", ["items"] = ForType(elementType, name) };

        throw new ArgumentException(
            $"Unsupported tool parameter type {type} for parameter '{name}'. Supported types: string, bool, " +
            "integer and floating-point types, enums, their nullable versions, arrays and lists of those, " +
            "and dictionaries with string keys.");
    }

    private static Type? ElementType(Type type)
    {
        if (type.IsArray)
            return type.GetElementType();
        if (!type.IsGenericType)
            return null;
        var definition = type.GetGenericTypeDefinition();
        return definition == typeof(List<>) || definition == typeof(IList<>) || definition == typeof(IReadOnlyList<>)
            || definition == typeof(IEnumerable<>) || definition == typeof(ICollection<>)
            || definition == typeof(IReadOnlyCollection<>) || definition == typeof(HashSet<>) || definition == typeof(ISet<>)
            ? type.GetGenericArguments()[0]
            : null;
    }

    private static Type? DictionaryValueType(Type type)
    {
        if (!type.IsGenericType)
            return null;
        var definition = type.GetGenericTypeDefinition();
        if (definition != typeof(Dictionary<,>) && definition != typeof(IDictionary<,>)
            && definition != typeof(IReadOnlyDictionary<,>))
            return null;
        var arguments = type.GetGenericArguments();
        return arguments[0] == typeof(string)
            ? arguments[1]
            : throw new ArgumentException($"Dictionary tool parameters must have string keys, not {arguments[0]}.");
    }
}
