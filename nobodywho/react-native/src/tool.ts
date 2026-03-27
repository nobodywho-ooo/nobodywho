import {
  Tool as GeneratedTool,
  type ToolCallback,
  type ToolParameter,
} from "../generated/ts/nobodywho";

/**
 * Convert a value from JSON to the appropriate JS type
 * based on the parameter type string.
 */
function convertValue(value: unknown, paramType: string): unknown {
  switch (paramType) {
    case "int":
    case "integer":
    case "float":
    case "number":
    case "double":
      return Number(value);
    case "bool":
    case "boolean":
      return typeof value === "string" ? value === "true" : Boolean(value);
    case "string":
    default:
      return String(value);
  }
}

/**
 * Create a tool that the model can call during inference.
 *
 * Parameters are defined as an ordered list of `[name, type]` pairs.
 * The order determines how the parsed JSON arguments are passed as
 * positional arguments to the `call` function.
 *
 * Supported types: `"string"`, `"integer"`, `"number"`, `"boolean"`.
 *
 * @example
 * ```typescript
 * const weatherTool = createTool({
 *   name: "get_weather",
 *   description: "Get the current weather for a city",
 *   parameters: [["city", "string"], ["unit", "string"]],
 *   call: (city, unit) => JSON.stringify({ temp: 22, unit }),
 * });
 * ```
 */
export function createTool(opts: {
  name: string;
  description: string;
  parameters: [string, string][];
  call: (...args: unknown[]) => string;
}): GeneratedTool {
  const callback: ToolCallback = {
    call(argumentsJson: string): string {
      const parsed = JSON.parse(argumentsJson);
      const args = opts.parameters.map(([paramName, paramType]) =>
        convertValue(parsed[paramName], paramType)
      );
      return opts.call(...args);
    },
  };

  const toolParams: ToolParameter[] = opts.parameters.map(
    ([name, type]) => ({ name, type })
  );

  return new GeneratedTool(opts.name, opts.description, toolParams, callback);
}
