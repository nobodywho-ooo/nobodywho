import {
  RustTool,
  type RustToolCallback,
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
 * A tool that the model can call during inference.
 *
 * Wraps the internal RustTool with an ergonomic API that handles
 * JSON parsing and typed argument dispatch.
 *
 * @example
 * ```typescript
 * const weatherTool = new Tool({
 *   name: "get_weather",
 *   description: "Get the current weather for a city",
 *   parameters: [["city", "string"], ["unit", "string"]],
 *   call: (city, unit) => JSON.stringify({ temp: 22, unit }),
 * });
 * ```
 */
export class Tool {
  /** @internal */
  readonly _inner: RustTool;

  constructor(opts: {
    name: string;
    description: string;
    parameters: [string, string][];
    call: (...args: unknown[]) => string;
  }) {
    const callback: RustToolCallback = {
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

    this._inner = new RustTool(opts.name, opts.description, toolParams, callback);
  }

  /** Get the JSON schema for this tool's parameters. */
  getSchemaJson(): string {
    return this._inner.getSchemaJson();
  }
}
