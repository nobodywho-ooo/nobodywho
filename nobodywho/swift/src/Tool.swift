import Foundation
import NobodyWhoGenerated

/// A tool that the model can call during inference.
///
/// The easiest way to create a tool is with the `@DeclareTool` macro.
/// It works with both sync and async functions:
/// ```swift
/// @DeclareTool("Get the current weather for a city")
/// func getWeather(city: String, unit: String) -> String {
///     return "{\"temp\": 22, \"unit\": \"\(unit)\"}"
/// }
///
/// @DeclareTool("Search the knowledge base")
/// func search(query: String) async -> String {
///     return await knowledgeBase.search(query)
/// }
/// ```
///
/// You can also create tools manually:
/// ```swift
/// let weatherTool = Tool(
///     name: "get_weather",
///     description: "Get the current weather for a city",
///     parameters: [("city", "string"), ("unit", "string")]
/// ) { args in
///     let city = args[0] as! String
///     let unit = args[1] as! String
///     return "{\"temp\": 22, \"unit\": \"\(unit)\"}"
/// }
/// ```
public class Tool {
    let inner: RustTool

    /// Create a tool with a synchronous callback.
    public init(
        name: String,
        description: String,
        parameters: [(String, String)],
        call: @escaping ([Any]) -> String
    ) {
        let callback = ToolCallbackImpl(parameters: parameters, call: call)
        let toolParams = parameters.map { ToolParameter(name: $0.0, schema: "{\"type\": \"\($0.1)\"}") }
        self.inner = RustTool(name: name, description: description, parameters: toolParams, callback: callback)
    }

    /// Create a tool with an async callback.
    public init(
        name: String,
        description: String,
        parameters: [(String, String)],
        call: @escaping ([Any]) async -> String
    ) {
        let callback = AsyncToolCallbackImpl(parameters: parameters, call: call)
        let toolParams = parameters.map { ToolParameter(name: $0.0, schema: "{\"type\": \"\($0.1)\"}") }
        self.inner = RustTool(name: name, description: description, parameters: toolParams, callback: callback)
    }

    /// Create a tool from ToolParameter values and a callback (used by @DeclareTool macro).
    public init(
        name: String,
        description: String,
        parameters: [ToolParameter],
        callback: RustToolCallback
    ) {
        self.inner = RustTool(name: name, description: description, parameters: parameters, callback: callback)
    }

    /// Get the JSON schema for this tool's parameters.
    public func getSchemaJson() -> String {
        return inner.getSchemaJson()
    }
}

/// A simple callback wrapper that bridges a closure to `RustToolCallback`.
/// Used by the `@DeclareTool` macro for sync functions.
public class ToolCallbackClosure: RustToolCallback {
    let handler: (String) -> String

    public init(_ handler: @escaping (String) -> String) {
        self.handler = handler
    }

    public func call(argumentsJson: String) -> String {
        return handler(argumentsJson)
    }
}

/// A callback wrapper that bridges an async closure to `RustToolCallback`.
/// Used by the `@DeclareTool` macro for async functions.
public class AsyncToolCallbackClosure: RustToolCallback {
    let handler: (String) async -> String

    public init(_ handler: @escaping (String) async -> String) {
        self.handler = handler
    }

    public func call(argumentsJson: String) -> String {
        let semaphore = DispatchSemaphore(value: 0)
        var result = "Error: async tool call did not complete"
        Task {
            result = await handler(argumentsJson)
            semaphore.signal()
        }
        semaphore.wait()
        return result
    }
}

// MARK: - Private helpers

private func convertValue(_ value: Any, paramType: String) -> Any {
    switch paramType {
    case "int", "integer":
        if let num = value as? NSNumber { return num.intValue }
        if let str = value as? String { return Int(str) ?? 0 }
        return 0
    case "float", "number", "double":
        if let num = value as? NSNumber { return num.doubleValue }
        if let str = value as? String { return Double(str) ?? 0.0 }
        return 0.0
    case "bool", "boolean":
        if let b = value as? Bool { return b }
        if let str = value as? String { return str == "true" }
        return false
    case "string":
        return String(describing: value)
    default:
        return String(describing: value)
    }
}

private func parseArgs(_ argumentsJson: String, parameters: [(String, String)]) -> [Any]? {
    guard let data = argumentsJson.data(using: .utf8),
          let parsed = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
        return nil
    }
    return parameters.map { (paramName, paramType) -> Any in
        guard let value = parsed[paramName] else { return NSNull() }
        return convertValue(value, paramType: paramType)
    }
}

/// Sync callback implementation for the manual Tool API.
private class ToolCallbackImpl: RustToolCallback {
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
private class AsyncToolCallbackImpl: RustToolCallback {
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
