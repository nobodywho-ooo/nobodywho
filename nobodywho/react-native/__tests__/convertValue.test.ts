import { _convertValue as convertValue } from "../src/tool";

describe("convertValue", () => {
  // --- Primitives ---

  test("converts to number", () => {
    expect(convertValue("42", { type: "number" })).toBe(42);
    expect(convertValue("3.14", { type: "number" })).toBe(3.14);
    expect(convertValue(7, { type: "number" })).toBe(7);
  });

  test("converts to integer", () => {
    expect(convertValue("42", { type: "integer" })).toBe(42);
    expect(convertValue(42, { type: "integer" })).toBe(42);
  });

  test("converts to boolean", () => {
    expect(convertValue("true", { type: "boolean" })).toBe(true);
    expect(convertValue("false", { type: "boolean" })).toBe(false);
    expect(convertValue(true, { type: "boolean" })).toBe(true);
    expect(convertValue(false, { type: "boolean" })).toBe(false);
  });

  test("converts to string", () => {
    expect(convertValue("hello", { type: "string" })).toBe("hello");
    expect(convertValue(42, { type: "string" })).toBe("42");
  });

  // --- Null / undefined passthrough ---

  test("passes through null and undefined", () => {
    expect(convertValue(null, { type: "string" })).toBeNull();
    expect(convertValue(undefined, { type: "number" })).toBeUndefined();
  });

  // --- Arrays ---

  test("converts array of numbers", () => {
    expect(
      convertValue([1, "2", "3.5"], { type: "array", items: { type: "number" } }),
    ).toEqual([1, 2, 3.5]);
  });

  test("converts array of strings", () => {
    expect(
      convertValue(["a", 1, true], { type: "array", items: { type: "string" } }),
    ).toEqual(["a", "1", "true"]);
  });

  test("converts array of booleans", () => {
    expect(
      convertValue(["true", "false", true], {
        type: "array",
        items: { type: "boolean" },
      }),
    ).toEqual([true, false, true]);
  });

  test("returns value as-is if not an array", () => {
    expect(
      convertValue("not an array", { type: "array", items: { type: "number" } }),
    ).toBe("not an array");
  });

  test("returns array as-is if no items schema", () => {
    expect(convertValue([1, 2, 3], { type: "array" })).toEqual([1, 2, 3]);
  });

  // --- Objects ---

  test("converts nested object properties", () => {
    const schema = {
      type: "object",
      properties: {
        lat: { type: "number" },
        lon: { type: "number" },
        name: { type: "string" },
      },
    };
    expect(
      convertValue({ lat: "51.5", lon: "-0.12", name: "London" }, schema),
    ).toEqual({ lat: 51.5, lon: -0.12, name: "London" });
  });

  test("sets missing properties to undefined", () => {
    const schema = {
      type: "object",
      properties: {
        a: { type: "string" },
        b: { type: "number" },
      },
    };
    expect(convertValue({ a: "hello" }, schema)).toEqual({
      a: "hello",
      b: undefined,
    });
  });

  test("returns value as-is if not an object", () => {
    const schema = {
      type: "object",
      properties: { a: { type: "string" } },
    };
    expect(convertValue("not an object", schema)).toBe("not an object");
  });

  test("returns object as-is if no properties schema", () => {
    expect(convertValue({ a: 1 }, { type: "object" })).toEqual({ a: 1 });
  });

  // --- Nested / recursive ---

  test("converts array of objects", () => {
    const schema = {
      type: "array",
      items: {
        type: "object",
        properties: {
          city: { type: "string" },
          temp: { type: "number" },
        },
      },
    };
    expect(
      convertValue(
        [
          { city: "Oslo", temp: "12" },
          { city: "London", temp: "15" },
        ],
        schema,
      ),
    ).toEqual([
      { city: "Oslo", temp: 12 },
      { city: "London", temp: 15 },
    ]);
  });

  test("converts object with nested array", () => {
    const schema = {
      type: "object",
      properties: {
        name: { type: "string" },
        scores: { type: "array", items: { type: "number" } },
      },
    };
    expect(
      convertValue({ name: "test", scores: ["1", "2", "3"] }, schema),
    ).toEqual({ name: "test", scores: [1, 2, 3] });
  });

  test("converts deeply nested structure", () => {
    const schema = {
      type: "object",
      properties: {
        route: {
          type: "object",
          properties: {
            origin: { type: "string" },
            stops: {
              type: "array",
              items: {
                type: "object",
                properties: {
                  city: { type: "string" },
                  duration: { type: "integer" },
                },
              },
            },
          },
        },
      },
    };
    expect(
      convertValue(
        {
          route: {
            origin: "Oslo",
            stops: [
              { city: "Stockholm", duration: "3" },
              { city: "Helsinki", duration: "2" },
            ],
          },
        },
        schema,
      ),
    ).toEqual({
      route: {
        origin: "Oslo",
        stops: [
          { city: "Stockholm", duration: 3 },
          { city: "Helsinki", duration: 2 },
        ],
      },
    });
  });

  test("converts triple-nested array", () => {
    const schema = {
      type: "array",
      items: {
        type: "array",
        items: {
          type: "array",
          items: { type: "number" },
        },
      },
    };
    expect(
      convertValue(
        [
          [["1", "2"], ["3"]],
          [["4", "5", "6"]],
        ],
        schema,
      ),
    ).toEqual([
      [[1, 2], [3]],
      [[4, 5, 6]],
    ]);
  });

  // --- Unknown type passthrough ---

  test("passes through unknown schema types", () => {
    expect(convertValue({ a: 1 }, { type: "unknown" })).toEqual({ a: 1 });
    expect(convertValue("hello", {})).toBe("hello");
  });
});
