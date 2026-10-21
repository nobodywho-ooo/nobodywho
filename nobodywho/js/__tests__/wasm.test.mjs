// Integration test of the WASM build, run with `npm run test:wasm`.
//
// Imports the package by name, so this goes through package.json `exports`
// (the `node` condition), and thereby tests the built `dist/`.
//
// The model must be accessible from inside Emscripten's virtual filesystem,
// e.g. by embedding it with `-Clink-arg=--embed-file=<host path>@<path>`
// (see DEVELOPMENT.md). Set `TEST_MODEL_WASM` to `<path>`.
import { test } from "node:test";
import assert from "node:assert/strict";

import { Model, Chat } from "react-native-nobodywho";

const modelPath = process.env.TEST_MODEL_WASM;

test("can chat", { skip: !modelPath && "TEST_MODEL_WASM not set" }, async () => {
  const model = await Model.load({ modelPath });

  const chat = new Chat({
    model,
    systemPrompt: "You are a helpful assistant.",
    templateVariables: { enable_thinking: false },
  });

  // Stream tokens
  const tokens = [];
  for await (const token of chat.ask("What is the capital of Denmark?")) {
    tokens.push(token);
  }
  assert.ok(tokens.length > 0);
  assert.match(tokens.join(""), /Copenhagen/);

  // Or get the full response
  const response = await chat.ask("And of Sweden?").completed();
  assert.match(response, /Stockholm/);
});
