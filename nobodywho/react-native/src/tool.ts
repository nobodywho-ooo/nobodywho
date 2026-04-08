import {
  RustTool,
  type RustToolCallback,
  type ToolParameter,
} from "../generated/ts/nobodywho";

/**
 * Convert a value from JSON based on the JSON Schema type.
 */
function convertValue(
  value: unknown,
  schema: Record<string, unknown>,
): unknown {
  switch (schema.type) {
    case "integer":
    case "number":
      return Number(value);
    case "boolean":
      return typeof value === "string" ? value === "true" : Boolean(value);
    case "string":
      return String(value);
    default:
      return value;
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
 *   parameters: {
 *     city: { type: "string" },
 *     unit: { type: "string", enum: ["celsius", "fahrenheit"] },
 *   },
 *   call: ({ city, unit }) => JSON.stringify({ temp: 22, unit }),
 * });
 * ```
 */
export class Tool {
  /** @internal */
  readonly _inner: RustTool;

  constructor(opts: {
    name: string;
    description: string;
    parameters: Record<string, Record<string, unknown>>;
    call: (args: Record<string, unknown>) => string;
  }) {
    const paramEntries = Object.entries(opts.parameters);

    const callback: RustToolCallback = {
      call(argumentsJson: string): string {
        const parsed = JSON.parse(argumentsJson);
        const args: Record<string, unknown> = {};
        for (const [paramName, schema] of paramEntries) {
          args[paramName] = convertValue(parsed[paramName], schema);
        }
        return opts.call(args);
      },
    };

    const toolParams: ToolParameter[] = paramEntries.map(
      ([name, schema]) => ({ name, schema: JSON.stringify(schema) }),
    );

    this._inner = new RustTool(opts.name, opts.description, toolParams, callback);
  }

  /** Get the JSON schema for this tool's parameters. */
  getSchemaJson(): string {
    return this._inner.getSchemaJson();
  }
}
