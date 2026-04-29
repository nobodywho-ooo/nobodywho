"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.createTool = createTool;
const index_js_1 = require("../index.js");

/**
 * Convert a value from JSON to the appropriate JS type
 * based on the parameter type string.
 */
function convertValue(value, paramType) {
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
 * ```javascript
 * const weatherTool = createTool({
 *   name: "get_weather",
 *   description: "Get the current weather for a city",
 *   parameters: [["city", "string"], ["unit", "string"]],
 *   call: (city, unit) => JSON.stringify({ temp: 22, unit }),
 * });
 * ```
 */
function createTool(opts) {
    const properties = {};
    const required = [];
    for (const [name, type] of opts.parameters) {
        let jsonType;
        switch (type) {
            case "int":
            case "integer":
                jsonType = "integer";
                break;
            case "float":
            case "number":
            case "double":
                jsonType = "number";
                break;
            case "bool":
            case "boolean":
                jsonType = "boolean";
                break;
            default:
                jsonType = "string";
                break;
        }
        properties[name] = { type: jsonType };
        required.push(name);
    }
    const schema = JSON.stringify({
        type: "object",
        properties,
        required,
    });

    const callback = (argumentsJson) => {
        const parsed = JSON.parse(argumentsJson);
        const args = opts.parameters.map(([paramName, paramType]) =>
            convertValue(parsed[paramName], paramType)
        );
        return opts.call(...args);
    };

    return new index_js_1.Tool(opts.name, opts.description, schema, callback);
}
