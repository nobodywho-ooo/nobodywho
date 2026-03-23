// Phase A2: Streaming validation spike

import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const { Model, Chat } = require('./nobodywho.node');

const modelPath = "/nix/store/a4q04w01a976d1y3fjd2b28mif7y038m-Qwen_Qwen3-0.6B-Q4_K_M.gguf";

async function main() {
    console.log("hasDiscreteGpu:", Model.hasDiscreteGpu());

    console.log("Loading model:", modelPath);
    const model = await Model.load(modelPath, true, null);
    console.log("Model loaded!");

    console.log("Creating chat...");
    const chat = new Chat(model, "You are a helpful assistant. Keep your responses brief.", 4096);
    console.log("Chat created!");

    console.log("\nAsking: Hello, what are you?\n");
    console.log("Response: ");

    const stream = chat.ask("Hello, what are you?");
    let token;
    while ((token = await stream.nextToken()) !== null) {
        process.stdout.write(token);
    }
    console.log("\n\nDone!");
}

main().catch(console.error);
