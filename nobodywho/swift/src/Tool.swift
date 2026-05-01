import Foundation
import NobodyWhoGenerated

/// A tool that the model can call during inference.
///
/// The easiest way to create a tool is with the ``DeclareTool(_:)`` macro
/// at top-level or type-member scope:
/// ```swift
/// @DeclareTool("Get the current weather for a city")
/// func getWeather(city: String, unit: String) -> String {
///     return "{\"temp\": 22, \"unit\": \"\(unit)\"}"
/// }
/// // Generates: let getWeatherTool: Tool = ...
/// ```
///
/// The macro does not work inside function bodies (a Swift peer macro limitation).
/// For local-scope tools, or when you need to capture local variables, use the
/// initializer directly with JSON Schema type strings:
/// ```swift
/// let weatherTool = Tool(
///     name: "get_weather",
///     description: "Get the current weather for a city",
///     parameters: [("city", #"{"type": "string"}"#), ("unit", #"{"type": "string"}"#)]
/// ) { args in
///     let city = args[0] as! String
///     let unit = args[1] as! String
///     return "{\"temp\": 22, \"unit\": \"\(unit)\"}"
/// }
/// ```
public class Tool {
    let inner: RustTool

    /// Create a tool with a synchronous callback.
    ///
    /// Each parameter is a `(name, schema)` tuple where `schema` is a JSON Schema string
    /// (e.g. `#"{"type": "string"}"#` or `#"{"type": "array", "items": {"type": "integer"}}"#`).
    public init(
        name: String,
        description: String,
        parameters: [(String, String)],
        call: @escaping ([Any]) -> String
    ) {
        let callback = ToolCallbackImpl(parameters: parameters, call: call)
        let toolParams = parameters.map { ToolParameter(name: $0.0, schema: $0.1) }
        self.inner = RustTool(name: name, description: description, parameters: toolParams, callback: callback)
    }

    /// Create a tool with an async callback.
    ///
    /// Each parameter is a `(name, schema)` tuple where `schema` is a JSON Schema string.
    public init(
        name: String,
        description: String,
        parameters: [(String, String)],
        call: @escaping ([Any]) async -> String
    ) {
        let callback = AsyncToolCallbackImpl(parameters: parameters, call: call)
        let toolParams = parameters.map { ToolParameter(name: $0.0, schema: $0.1) }
        self.inner = RustTool(name: name, description: description, parameters: toolParams, callback: callback)
    }

    /// Get the JSON schema for this tool's parameters.
    public func getSchemaJson() -> String {
        return inner.getSchemaJson()
    }
}

// MARK: - Private helpers

/// Recursively convert a JSON-deserialized value to a typed Swift value based on a JSON Schema.
private func convertValue(_ value: Any, schema: [String: Any]) -> Any {
    guard let type = schema["type"] as? String else {
        return String(describing: value)
    }

    switch type {
    case "string":
        return String(describing: value)
    case "integer":
        if let num = value as? NSNumber { return num.intValue }
        if let str = value as? String { return Int(str) ?? 0 }
        return 0
    case "number":
        if let num = value as? NSNumber { return num.doubleValue }
        if let str = value as? String { return Double(str) ?? 0.0 }
        return 0.0
    case "boolean":
        if let b = value as? Bool { return b }
        if let str = value as? String { return str == "true" }
        return false
    case "array":
        let itemSchema = schema["items"] as? [String: Any] ?? ["type": "string"]
        if let array = value as? [Any] {
            return array.map { convertValue($0, schema: itemSchema) }
        }
        return [Any]()
    case "object":
        let valueSchema = schema["additionalProperties"] as? [String: Any] ?? ["type": "string"]
        if let dict = value as? [String: Any] {
            return dict.mapValues { convertValue($0, schema: valueSchema) }
        }
        return [String: Any]()
    default:
        return String(describing: value)
    }
}

/// Parse a JSON Schema string into a dictionary. Returns a fallback string schema on failure.
private func parseSchema(_ schemaString: String) -> [String: Any] {
    guard let data = schemaString.data(using: .utf8),
          let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
        return ["type": "string"]
    }
    return obj
}

private func parseArgs(_ argumentsJson: String, parameters: [(String, String)]) -> [Any]? {
    guard let data = argumentsJson.data(using: .utf8),
          let parsed = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
        return nil
    }
    return parameters.map { (paramName, schemaString) -> Any in
        guard let value = parsed[paramName] else { return NSNull() }
        return convertValue(value, schema: parseSchema(schemaString))
    }
}

/// Sync callback implementation for the manual Tool API.
private final class ToolCallbackImpl: @unchecked Sendable, RustToolCallback {
    let parameters: [(String, String)]
    let callHandler: ([Any]) -> String

    init(parameters: [(String, String)], call: @escaping ([Any]) -> String) {
        self.parameters = parameters
        self.callHandler = call
    }

    func call(argumentsJson: String) -> String {
        guard let args = parseArgs(argumentsJson, parameters: parameters) else {
            return "Error: Failed to parse arguments JSON"
        }
        return callHandler(args)
    }
}

/// Async callback implementation for the manual Tool API.
private final class AsyncToolCallbackImpl: @unchecked Sendable, RustToolCallback {
    let parameters: [(String, String)]
    let callHandler: ([Any]) async -> String

    init(parameters: [(String, String)], call: @escaping ([Any]) async -> String) {
        self.parameters = parameters
        self.callHandler = call
    }

    func call(argumentsJson: String) -> String {
        guard let args = parseArgs(argumentsJson, parameters: parameters) else {
            return "Error: Failed to parse arguments JSON"
        }
        let semaphore = DispatchSemaphore(value: 0)
        var result = "Error: async tool call did not complete"
        Task {
            result = await callHandler(args)
            semaphore.signal()
        }
        semaphore.wait()
        return result
    }
}
