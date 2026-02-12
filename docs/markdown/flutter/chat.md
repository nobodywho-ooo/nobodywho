---
title: Chat
description: A concise introduction to the Chat functionality of NobodyWho.
sidebar_title: Chat
order: 1
---

As you may have noticed in the [welcome guide](./index.md), every interaction with your LLM starts by instantiating a `Chat` object.
In the following sections, we talk about which configuration options it has, and when to use them.w


## Creating a Chat 

There are two main ways of instantiating a `Chat` object and the difference lies in when the model file is loaded. 
The simplest way is using `Chat.fromPath` like so:

```dart 
final chat = await nobodywho.Chat.fromPath(modelPath: "./model.gguf");
```
This function is async since loading a model can take a bit of time, but this should not block the any of your UI.
Another way to achieve the same thing is to load the model seperately and then use the `Chat` constructor:

```dart
final model = await nobodywho.Model.load(modelPath: "./model.gguf");
final chat = nobodywho.Chat(model : model);
```

This allows for sharing the model between several `Chat` instances.

## Prompts and responses

The `Chat.ask()` function is central to NobodyWho. This function sends your message to the LLM, which then starts generating a response.

```dart
import "dart:io"
final chat = await nobodywho.Chat.fromPath(modelPath: "./model.gguf");
final response = await chat.ask("Is water wet?");
```

The return type of `ask` is a `TokenStream`.
If you want to start reading the response as soon as possible, you can just iterate over the `TokenStream`.
Each token is either an individual word or fragments of a word.

```{.dart continuation}
await for (final token in response) {
   stdout.write(token);
   await stdout.flush();
}
print("\n");
```

If you just want to get the complete response, you can call `TokenStream.completed()`.
This will return the entire response string once the model is done generating its entire response.

```{.dart continuation}
final fullResponse = await response.completed();
```

All of your messages and the model's responses are stored in the `Chat` object, so the next time you call `Chat.ask()`, it will remember the previous messages.

## Chat history

If you want to inspect the messages inside the `Chat` object, you can use `getChatHistory`.

```{.dart continuation}
final msgs = await chat.getChatHistory();
print(msgs[0].content); // "Is water wet?"
```

Similarly, if you want to edit what messages are in the context, you can use `setChatHistory`:

```{.dart continuation}
await chat.setChatHistory([
  nobodywho.Message.message(role: nobodywho.Role.user, content: "What is water?")
]);
```

## System prompt

A system prompt is a special message put into the chat context, which should guide its overall behavior.
Some models ship with a built-in system prompt. If you don't specify a system prompt yourself, NobodyWho will fall back to using the model's default system prompt.

You can specify a system prompt when initializing a `Chat`:

```dart
import 'package:nobodywho_dart/nobodywho_dart.dart' as nobodywho;

final chat = await nobodywho.Chat.fromPath(
  modelPath: "./model.gguf",
  systemPrompt: "You are a mischievous assistant!"
);
```

This `systemPrompt` is then persisted until the chat context is `reset`.

## Context

The context is the text window which the LLM currently considers. Specifically this is the number of tokens the LLM keeps in memory for your current conversation.
As bigger context size means more computational overhead, it makes sense to constrain it. This can be done with `contextSize` setting, again at the time of creation:

```dart
final chat = await nobodywho.Chat.fromPath(
  modelPath: "./model.gguf",
  contextSize: 4096
);
```

The default value is `4096`, however this is mainly useful for short and simple conversations. Choosing the right context size is quite important and depends heavily on your use case. A good place to start is to look at your selected models documentation and see what their recommended context size is.

Even with properly selected context size it might happen that you fill up your entire context during a conversation. When this happens, NobodyWho will shrink the context for you. Currently this is done by removing old messages (apart from the system prompt and the first user message) from the chat history, until the size reaches `contextSize / 2`. The KV cache is also updated automatically. In the future we plan on adding more advanced methods of context shrinking.

Again, `contextSize` is fixed to the `Chat` instance, so it is currently not possible to change the size after `Chat` is created. To reset the current context content, just call `resetContext()` with the new system prompt and potentially changed tools.

```{.dart continuation}
await chat.resetContext(systemPrompt: "New system prompt", tools: []);
```

If you don't want to change the already set defaults (`systemPrompt`, `tools`), but only reset the context, then go for `resetHistory`.

## Sharing model between contexts

There are scenarios where you would like to keep separate chat contexts (e.g. for every user of your app), but have only one model loaded. In this case you must load the model 
seperately from creating the `Chat` instance.

For this use case, instead of the path to the `.gguf` model, you can pass in `Model` object, which can be shared between multiple `Chat` instances.

```dart
import 'package:nobodywho_dart/nobodywho_dart.dart' as nobodywho;

final model = await nobodywho.Model.load(modelPath: './model.gguf');
final chat1 = nobodywho.Chat(model: model);
final chat2 = nobodywho.Chat(model: model);
...
```

NobodyWho will then take care of the separation, such that your chat histories won't collide or interfere with each other, while having only one model loaded.

## GPU
When instantiating `Model` or using `Chat.fromPath` you have the option to disable/enable GPU acceleration. This can be done as:
```dart
final model = await nobodywho.Model.load(modelPath: './model.gguf', useGpu: true);
```
or 
```dart
final chat = await nobodywho.Chat.fromPath(modelPath: './model.gguf', useGpu : false);
```
By defualt `useGpu` is set to true.
So far, NobodyWho relies purely on [Vulkan](https://www.vulkan.org), however support
of more architectures is planned (for details check out our [issues](https://github.com/nobodywho-ooo/nobodywho/issues) or join us on [Discord](https://discord.gg/qhaMc2qCYB)).

## Reasoning

To enhance cognitive performance, allowing reasoning can be a good idea:
```dart
final chat = await nobodywho.Chat.fromPath(
  modelPath: "./model.gguf",
  allowThinking: true
);
```
Note that `Chat` has thinking set to `true` by default. You can also change the setting of an already existing chat instance:
```{.dart continuation}
await chat.setAllowThinking(true);
```
With the next message send, the setting will be propagated.

!!! info ""
    Note that **not every model** supports reasoning. If the model does not have
    such an option, the `allowThinking` setting will be ignored.

