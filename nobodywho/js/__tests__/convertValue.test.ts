import { describe, test } from "node:test";
import assert from "node:assert/strict";
import { convertValue } from "../src/wrapper/convert_value.ts";

describe("convertValue", () => {
  // --- Primitives ---

  test("converts to number", () => {
    assert.equal(convertValue("42", { type: "number" }), 42);
    assert.equal(convertValue("3.14", { type: "number" }), 3.14);
    assert.equal(convertValue(7, { type: "number" }), 7);
  });

  test("converts to integer", () => {
    assert.equal(convertValue("42", { type: "integer" }), 42);
    assert.equal(convertValue(42, { type: "integer" }), 42);
  });

  test("converts to boolean", () => {
    assert.equal(convertValue("true", { type: "boolean" }), true);
    assert.equal(convertValue("false", { type: "boolean" }), false);
    assert.equal(convertValue(true, { type: "boolean" }), true);
    assert.equal(convertValue(false, { type: "boolean" }), false);
  });

  test("converts to string", () => {
    assert.equal(convertValue("hello", { type: "string" }), "hello");
    assert.equal(convertValue(42, { type: "string" }), "42");
  });

  // --- Null / undefined passthrough ---

  test("passes through null and undefined", () => {
    assert.equal(convertValue(null, { type: "string" }), null);
    assert.equal(convertValue(undefined, { type: "number" }), undefined);
  });

  // --- Arrays ---

  test("converts array of numbers", () => {
    assert.deepEqual(
      convertValue([1, "2", "3.5"], {
        type: "array",
        items: { type: "number" },
      }),
      [1, 2, 3.5],
    );
  });

  test("converts array of strings", () => {
    assert.deepEqual(
      convertValue(["a", 1, true], {
        type: "array",
        items: { type: "string" },
      }),
      ["a", "1", "true"],
    );
  });

  test("converts array of booleans", () => {
    assert.deepEqual(
      convertValue(["true", "false", true], {
        type: "array",
        items: { type: "boolean" },
      }),
      [true, false, true],
    );
  });

  test("returns value as-is if not an array", () => {
    assert.equal(
      convertValue("not an array", {
        type: "array",
        items: { type: "number" },
      }),
      "not an array",
    );
  });

  test("returns array as-is if no items schema", () => {
    assert.deepEqual(convertValue([1, 2, 3], { type: "array" }), [1, 2, 3]);
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
    assert.deepEqual(
      convertValue({ lat: "51.5", lon: "-0.12", name: "London" }, schema),
      { lat: 51.5, lon: -0.12, name: "London" },
    );
  });

  test("sets missing properties to undefined", () => {
    const schema = {
      type: "object",
      properties: {
        a: { type: "string" },
        b: { type: "number" },
      },
    };
    assert.deepEqual(convertValue({ a: "hello" }, schema), {
      a: "hello",
      b: undefined,
    });
  });

  test("returns value as-is if not an object", () => {
    const schema = {
      type: "object",
      properties: { a: { type: "string" } },
    };
    assert.equal(convertValue("not an object", schema), "not an object");
  });

  test("returns object as-is if no properties schema", () => {
    assert.deepEqual(convertValue({ a: 1 }, { type: "object" }), { a: 1 });
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
    assert.deepEqual(
      convertValue(
        [
          { city: "Oslo", temp: "12" },
          { city: "London", temp: "15" },
        ],
        schema,
      ),
      [
        { city: "Oslo", temp: 12 },
        { city: "London", temp: 15 },
      ],
    );
  });

  test("converts object with nested array", () => {
    const schema = {
      type: "object",
      properties: {
        name: { type: "string" },
        scores: { type: "array", items: { type: "number" } },
      },
    };
    assert.deepEqual(
      convertValue({ name: "test", scores: ["1", "2", "3"] }, schema),
      { name: "test", scores: [1, 2, 3] },
    );
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
    assert.deepEqual(
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
      {
        route: {
          origin: "Oslo",
          stops: [
            { city: "Stockholm", duration: 3 },
            { city: "Helsinki", duration: 2 },
          ],
        },
      },
    );
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
    assert.deepEqual(
      convertValue([[["1", "2"], ["3"]], [["4", "5", "6"]]], schema),
      [[[1, 2], [3]], [[4, 5, 6]]],
    );
  });

  // --- Unknown type passthrough ---

  test("passes through unknown schema types", () => {
    assert.deepEqual(convertValue({ a: 1 }, { type: "unknown" }), { a: 1 });
    assert.equal(convertValue("hello", {}), "hello");
  });
});
