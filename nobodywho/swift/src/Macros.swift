/// Generates a `Tool` instance from a function declaration.
///
/// The macro inspects the function's parameter names and types to automatically
/// build the tool's parameter schema and argument parsing.
///
/// Usage:
/// ```swift
/// @DeclareTool("Get the current weather for a city")
/// func getWeather(city: String, unit: String) -> String {
///     return "{\"temp\": 22, \"unit\": \"\(unit)\"}"
/// }
/// // Generates: let getWeatherTool: Tool = ...
/// ```
///
/// The generated variable is named `<functionName>Tool`. For example,
/// `getWeather` becomes `getWeatherTool`.
///
/// Supported parameter types: `String`, `Int`, `Double`, `Float`, `Bool`.
@attached(peer, names: arbitrary)
public macro DeclareTool(_ description: String) = #externalMacro(module: "NobodyWhoMacros", type: "ToolMacro")
