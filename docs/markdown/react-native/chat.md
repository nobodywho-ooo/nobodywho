---
title: Chat
description: A concise introduction to the Chat functionality of NobodyWho.
sidebar_title: Chat
order: 1
---

As you may have noticed in the [welcome guide](./index.md), every interaction with your LLM starts by creating a `Chat` object.
In the following sections, we talk about which configuration options it has, and when to use them.

## Creating a Chat

There are two main ways of creating a `Chat` object and the difference lies in when the model file is loaded.
The simplest way is using `Chat.fromPath`:

```typescript
import { Chat } from "react-native-nobodywho";

const chat = await Chat.fromPath({ modelPath: "/path/to/model.gguf" });
```

This function is async since loading a model can take a bit of time, but this should not block any of your UI.
Another way to achieve the same thing is to load the model separately and then use the `Chat` constructor:

```typescript
import { Model, Chat } from "react-native-nobodywho";

const model = await Model.load({ modelPath: "/path/to/model.gguf" });
const chat = new Chat({ model });
```

This allows for sharing the model between several `Chat` instances.

## Prompts and responses

The `chat.ask()` function is central to NobodyWho. This function sends your message to the LLM, which then starts generating a response.

```typescript
const chat = await Chat.fromPath({ modelPath: "/path/to/model.gguf" });
const response = chat.ask("Is water wet?");
```

The return type of `ask` is a `TokenStream`.
If you want to start reading the response as soon as possible, you can iterate over the `TokenStream` using `for await`.
Each token is either an individual word or a fragment of a word.

```typescript
for await (const token of response) {
  process.stdout.write(token);
}
```

If you just want to get the complete response, you can call `completed()`.
This will return the entire response string once the model is done generating.

```typescript
const fullResponse = await response.completed();
```

All of your messages and the model's responses are stored in the `Chat` object, so the next time you call `chat.ask()`, it will remember the previous messages.

## Chat history

If you want to inspect the messages inside the `Chat` object, you can use `getChatHistory`.

```typescript
const msgs = await chat.getChatHistory();
console.log(msgs[0]); // The first message
```

Similarly, if you want to edit what messages are in the context, you can use `setChatHistory`:

```typescript
import { Role } from "react-native-nobodywho";

await chat.setChatHistory([
  { role: Role.User, content: "What is water?" },
]);
```

## System prompt

A system prompt is a special message put into the chat context, which should guide its overall behavior.
Some models ship with a built-in system prompt. If you don't specify a system prompt yourself, NobodyWho will fall back to using the model's default system prompt.

You can specify a system prompt when creating a `Chat`:

```typescript
import { Chat } from "react-native-nobodywho";

const chat = await Chat.fromPath({
  modelPath: "/path/to/model.gguf",
  systemPrompt: "You are a mischievous assistant!",
});
```

This `systemPrompt` is then persisted until the chat context is reset.

## Context

The context is the text window which the LLM currently considers. Specifically this is the number of tokens the LLM keeps in memory for your current conversation.
A bigger context size means more computational overhead, so it makes sense to constrain it. This can be done with the `contextSize` setting at creation time:

```typescript
const chat = await Chat.fromPath({
  modelPath: "/path/to/model.gguf",
  contextSize: 4096,
});
```

The default value is `4096`, however this is mainly useful for short and simple conversations. Choosing the right context size is quite important and depends heavily on your use case. A good place to start is to look at your selected model's documentation and see what their recommended context size is.

Even with a properly selected context size it might happen that you fill up your entire context during a conversation. When this happens, NobodyWho will shrink the context for you. Currently this is done by removing old messages (apart from the system prompt and the first user message) from the chat history, until the size reaches `contextSize / 2`. The KV cache is also updated automatically.

To reset the current context content, call `resetContext()` with a new system prompt and potentially changed tools.

```typescript
await chat.resetContext({ systemPrompt: "New system prompt", tools: [] });
```

If you don't want to change the already set defaults (`systemPrompt`, `tools`), but only reset the context, then go for `resetHistory`.

## Sharing model between contexts

There are scenarios where you would like to keep separate chat contexts (e.g. for every user of your app), but have only one model loaded. In this case you must load the model separately from creating the `Chat` instance.

```typescript
import { Model, Chat } from "react-native-nobodywho";

const model = await Model.load({ modelPath: "/path/to/model.gguf" });
const chat1 = new Chat({ model });
const chat2 = new Chat({ model });
```

NobodyWho will then take care of the separation, such that your chat histories won't collide or interfere with each other, while having only one model loaded.

## GPU

When using `Model.load` or `Chat.fromPath` you have the option to disable/enable GPU acceleration:

```typescript
const model = await Model.load({ modelPath: "/path/to/model.gguf", useGpu: false });
```

or

```typescript
const chat = await Chat.fromPath({
  modelPath: "/path/to/model.gguf",
  useGpu: false,
});
```

By default `useGpu` is set to `true`.

## Template Variables

Chat templates are used internally by models to format conversation history into the expected prompt format. Different models may support different template variables that control specific behaviors. Template variables are boolean flags passed to the chat template that can enable or disable certain features.

### Using Template Variables

You can set template variables when creating a chat or modify them on existing instances:

```typescript
const chat = await Chat.fromPath({
  modelPath: "/path/to/model.gguf",
  templateVariables: new Map([["enable_thinking", true]]),
});
```

You can also modify template variables on an existing chat instance:

```typescript
// Set a single template variable
await chat.setTemplateVariable("enable_thinking", true);

// Get current template variables
const variables = await chat.getTemplateVariables();
console.log(variables); // Map { "enable_thinking" => true }
```

With the next message sent, the updated settings will be propagated to the model.

### Example: Qwen3 and Qwen3.5 Reasoning

The Qwen3 and Qwen3.5 model families support the `enable_thinking` template variable, which controls whether the model should engage in explicit reasoning steps before answering:

```typescript
const chat = await Chat.fromPath({
  modelPath: "/path/to/model.gguf",
  templateVariables: new Map([["enable_thinking", true]]),
});
const response = chat.ask("Solve this logic puzzle: ...");
```

When `enable_thinking` is enabled, these models will show their reasoning process before providing the final answer.

### Model-Specific Variables

Different models may support different template variables depending on their chat template implementation. The available variables and their effects depend entirely on how the model's chat template is designed. Check your model's documentation to see which template variables are supported.

!!! info ""
    Note that template variables are model-specific. If a model's chat template doesn't use a specific variable, that variable will be ignored gracefully.
