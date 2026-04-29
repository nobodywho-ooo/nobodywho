import type { Tool } from "../index.js";
/**
 * Create a tool that the model can call during inference.
 *
 * Parameters are defined as an ordered list of `[name, type]` pairs.
 * The order determines how the parsed JSON arguments are passed as
 * positional arguments to the `call` function.
 *
 * Supported types: `"string"`, `"integer"`, `"number"`, `"boolean"`.
 */
export declare function createTool(opts: {
    name: string;
    description: string;
    parameters: [string, string][];
    call: (...args: unknown[]) => string;
}): Tool;
