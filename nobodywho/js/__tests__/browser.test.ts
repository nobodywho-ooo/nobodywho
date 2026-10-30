// Integration test of the WASM build in Chrome, run with `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createReadStream, readFileSync } from "node:fs";
import { createServer } from "node:http";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import type { AddressInfo } from "node:net";

import { chromium } from "playwright-core";

const modelPath = process.env.TEST_MODEL;
const packageDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");

// `dist/index.mjs` leaves its dependencies as bare imports. Resolve them like
// Node.js would, and tell the browser to load the same files from `node_modules`.
const imports = Object.fromEntries(
  [...readFileSync(join(packageDir, "dist/index.mjs"), "utf8").matchAll(/from "([^".:][^"]*)"/g)].map(
    ([, specifier]) => [specifier, "/" + relative(packageDir, fileURLToPath(import.meta.resolve(specifier)))],
  ),
);
const page = `<!doctype html><script type="importmap">${JSON.stringify({ imports })}</script>`;

test("can chat in the browser", { skip: !modelPath && "TEST_MODEL not set" }, async (t) => {
  // Serves the package directory and the model.
  const server = createServer((req, res) => {
    const path = new URL(req.url!, "http://localhost").pathname;
    if (path === "/") {
      res.setHeader("content-type", "text/html").end(page);
    } else {
      const file = path === "/model.gguf" ? modelPath! : join(packageDir, path);
      const type = file.endsWith(".wasm") ? "application/wasm" : "text/javascript";
      createReadStream(file)
        .on("error", () => res.writeHead(404).end())
        .once("open", () => res.setHeader("content-type", type))
        .pipe(res);
    }
  });
  await new Promise<void>((resolve) => server.listen(0, resolve));
  t.after(() => server.close());
  const origin = `http://localhost:${(server.address() as AddressInfo).port}`;

  // Uses the installed Google Chrome.
  const browser = await chromium.launch({ channel: "chrome" });
  t.after(() => browser.close());
  const tab = await browser.newPage();
  tab.on("console", (msg) => t.diagnostic(`[browser] ${msg.text()}`));
  await tab.goto(origin);

  const { tokens, response } = await tab.evaluate(async (origin) => {
    const { Model, Chat } = await import(`${origin}/dist/index.mjs`);
    const model = await Model.load({ modelPath: `${origin}/model.gguf` });
    const chat = new Chat({
      model,
      systemPrompt: "You are a helpful assistant.",
      templateVariables: { enable_thinking: false },
    });

    const tokens: string[] = [];
    for await (const token of await chat.ask("What is the capital of Denmark?")) {
      tokens.push(token);
    }
    const response: string = await (await chat.ask("And of Sweden?")).completed();
    return { tokens, response };
  }, origin);

  assert.match(tokens.join(""), /Copenhagen/);
  assert.match(response, /Stockholm/);
});
