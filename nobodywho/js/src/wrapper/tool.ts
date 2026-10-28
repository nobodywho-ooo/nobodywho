import {
  RustTool,
  type ToolParameter,
} from "../generated";
import { convertValue } from "./convert_value";

/**
 * A tool that the model can call during inference.
 *
 * Wraps the internal RustTool with an ergonomic API that handles
 * JSON parsing and typed argument dispatch. Both sync and async
 * callbacks are supported.
 *
 * @example
 * ```typescript
 * // Any function can be used as a tool — arguments are passed positionally
 * // in the same order as the parameters object.
 * function getWeather(city: string, unit: string): string {
 *   return JSON.stringify({ temp: 22, unit });
 * }
 *
 * const tool = new Tool({
 *   name: "get_weather",
 *   description: "Get the current weather for a city",
 *   parameters: [
 *     { name: "city", type: "string" },
 *     { name: "unit", type: "string", enum: ["celsius", "fahrenheit"] },
 *   ],
 *   call: getWeather,
 * });
 *
 * // Async functions work too
 * const tool = new Tool({
 *   name: "search",
 *   description: "Search the knowledge base",
 *   parameters: [{ name: "query", type: "string" }],
 *   call: async (query) => await searchDatabase(query as string),
 * });
 * ```
 */
export class Tool {
  /** @internal */
  readonly _inner: RustTool;

  constructor(opts: {
    name: string;
    description: string;
    parameters: { name: string; [schemaKey: string]: unknown }[];
    call: (...args: any[]) => string | Promise<string>;
  }) {
    const params = opts.parameters;
    const toolParams: ToolParameter[] = params.map(({ name, ...schema }) => ({
      name,
      schema: JSON.stringify(schema),
    }));

    function parseArgs(argumentsJson: string): unknown[] {
      const parsed = JSON.parse(argumentsJson);
      return params.map(({ name, ...schema }) =>
        convertValue(parsed[name], schema),
      );
    }

    // Use the async constructor — the polling loop handles both sync
    // and async callbacks uniformly via await.
    this._inner = RustTool.newAsync(opts.name, opts.description, toolParams) as RustTool;

    // Start the polling loop. It exits when the RustTool is dropped
    // (nextPendingCall returns null).
    const poll = async () => {
      for (;;) {
        const call = await this._inner.nextPendingCall();
        if (call == null) break;
        try {
          const args = parseArgs(call.argumentsJson);
          const result = await opts.call(...args);
          this._inner.resolvePendingCall(call.callId, result);
        } catch (e) {
          this._inner.resolvePendingCall(
            call.callId,
            `Error: ${e instanceof Error ? e.message : String(e)}`,
          );
        }
      }
    };
    poll();
  }

  /** Get the JSON schema for this tool's parameters. */
  getSchemaJson(): string {
    return this._inner.getSchemaJson();
  }
}
