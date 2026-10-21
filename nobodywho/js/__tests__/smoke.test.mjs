import { test } from "node:test";
import assert from "node:assert/strict";

import { Model, Chat } from "nobodywho";

test("can load Model", async () => {
    const model = await Model.load({ modelPath: "/Qwen_Qwen3-0.6B-Q4_K_M.gguf" });

    const chat = new Chat({
        model,
        systemPrompt: "You are a helpful assistant.",
    });

    // Stream tokens
    for await (const token of chat.ask("Is water wet?")) {
        console.log(token);
    }

    // Or get the full response
    const response = await chat.ask("Is water wet?").completed();
});
