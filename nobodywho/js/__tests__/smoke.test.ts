// Integration test of the WASM build, run with `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";

import { Model, Chat } from "react-native-nobodywho";

const modelPath = process.env.TEST_MODEL;

test("can chat", { skip: !modelPath && "TEST_MODEL not set" }, async () => {
  const model = await Model.load({ modelPath });

  const chat = new Chat({
    model,
    systemPrompt: "You are a helpful assistant.",
    templateVariables: { enable_thinking: false },
  });

  // Stream tokens
  const tokens = [];
  for await (const token of await chat.ask("What is the capital of Denmark?")) {
    tokens.push(token);
  }
  assert.ok(tokens.length > 0);
  assert.match(tokens.join(""), /Copenhagen/);

  // Or get the full response
  const stream = await chat.ask("And of Sweden?");
  const response = await stream.completed();
  assert.match(response, /Stockholm/);
});
