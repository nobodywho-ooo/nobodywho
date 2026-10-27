/**
 * Recursively convert a value from JSON based on a JSON Schema type.
 *
 * Supports primitives (`string`, `number`, `integer`, `boolean`),
 * arrays (`{ type: "array", items: ... }`), and nested objects
 * (`{ type: "object", properties: { ... } }`).
 */
export function convertValue(
  value: unknown,
  schema: Record<string, unknown>,
): unknown {
  if (value === null || value === undefined) {
    return value;
  }

  switch (schema.type) {
    case "integer":
    case "number":
      return Number(value);
    case "boolean":
      return typeof value === "string" ? value === "true" : Boolean(value);
    case "string":
      return String(value);
    case "array": {
      if (!Array.isArray(value)) return value;
      const itemSchema = schema.items as Record<string, unknown> | undefined;
      if (!itemSchema) return value;
      return value.map((item) => convertValue(item, itemSchema));
    }
    case "object": {
      if (typeof value !== "object" || Array.isArray(value)) return value;
      const properties = schema.properties as
        | Record<string, Record<string, unknown>>
        | undefined;
      if (!properties) return value;
      const obj = value as Record<string, unknown>;
      const result: Record<string, unknown> = {};
      for (const [key, propSchema] of Object.entries(properties)) {
        result[key] = key in obj ? convertValue(obj[key], propSchema) : undefined;
      }
      return result;
    }
    default:
      return value;
  }
}

