---
title: LLM Basics
description: Essential concepts for working with language models in NobodyWho
sidebar_position: 2
---

Our goal with NobodyWho is to make it easy to run local LLMs. For this reason we have made it possible to use NobodyWho with minimal knowledge of how LLM works. However you still need to know some basic concepts, so for these we provide some brief explanations. The concepts covered are tokens, context, samplers, tools, and thinking/reasoning. 

## Tokens

Tokens are the basic units that LLMs process. A token is typically a word, part of a word, or a punctuation mark. For example, "hello" is one token, while "understanding" might be split into two tokens: "understand" and "ing". It is worth noting that the vocabulary of tokens used is different for each model as it is defined during training. 

When the model generates text, it produces one token at a time. This is why the default response object of NobodyWho is a stream of tokens and why you can read the response token-by-token.

## Context

Context refers to all the text the model can "see" when generating a response. This includes:
- Previous messages in the conversation
- The current user prompt
- Any system instructions

Essentially the context acts as the models memory of the current conversation, available tools etc. This is important to remember as once your chosen model has been initialized most of  your interactions with the model will happen through the context.

### Context Size

Every model has a maximum context size (also called context window or context length), measured in tokens. Common sizes range from 2048 to 128,000 tokens.

Once you reach the context limit, you must either:
- Start a new conversation
- Remove old messages from the history
- Summarize earlier parts of the conversation

Currently NobodyWho resolves this issue automatically by removing old messages from the context.
Having a larger context allows for longer and more complex conversations, but it also slows down the response time, as the model has to process more tokens each time it generates a response.

## Samplers

LLMs don't output text directly. Instead, they generate a probability distribution over all possible next tokens. Since the model weights are static after training, this means that the same input tokens always generate the same distribution. Depending on the use case however, there are many possible ways of choosing a next token from this distribution. This is configured using a **sampler**. A **sampler** splits the process of choosing a next token into two parts: Shifting the distribution and Sampling the distribution.

### Shifting the Distribution
Before sampling the distribution to get the next token, it is possible to adjust the distribution provided by the LLM to encourage certain behavior. Examples of these adjustments are:

- **Temperature**: Higher values make output more creative/random, lower values make it more focused/deterministic.
- **Top-k/Top-p**: Limit which tokens are considered, filtering out unlikely options
- **Penalties**: Lower the probalities of tokens already present in the context.

It is important to note that the steps in this part of the process can be chained. So it is possible to first apply a Temperature shift and then Top-k.


### Sampling the distribution
Once the distribution has been shifted the next step is to actually sample the distribution. This can also be done a few different ways:

- **Dist**: Sample the distribution randomly 
- **Greedy**: Always pick the most likely token (deterministic but sometimes repetitive)
- **Mirostat**: Advanced sampling presented in this [article](https://arxiv.org/abs/1904.09751)

Since this part actually chooses the next token, these cannot be chained.


NobodyWho also supports more advanced ways of configuraing a sampler, like for example follow a JSON Schema.

## Speculative decoding (MTP)

Speculative decoding is a way of making inference faster without changing the model's outputs. A small, cheap "draft" model proposes the next few tokens, and the real target model verifies them in one batch. Tokens that the target would have picked anyway are accepted for free — otherwise the target's own choice is used. Same distribution, fewer sequential forward passes.

NobodyWho supports the **MTP** (Multi-Token Prediction) flavour used by Gemma 4: instead of running a separate draft model, MTP uses extra "heads" trained to predict several tokens ahead of the target. You download the MTP heads as a companion `.gguf` (e.g. `mtp-gemma-4-E2B-it.gguf` for Gemma-4-E2B), pass it as `draft_model_path` when loading the model, and set `mtp = true` on the chat.

Expect a large speedup on structured/deterministic output (code, JSON, math, tool calls) and a neutral to modest speedup on high-entropy prose. MTP is also highly hardware-dependent — you will get the most out of it when memory bandwidth is the biggest bottleneck during generation.
This means that on systems with unified memory (e.g. a Mac mini), the speedup might not be very big and you might even get worse performance. For this reason it is important to be mindful about both your hardware and your use case. The best option is of course to benchmark it yourself.
Enabling MTP adds around 5% to VRAM usage. It is off by default.

## Tools

Tools (also called function calling) allow the LLM to request external actions. Instead of just generating text, the model can indicate it wants to:
- Search a database
- Perform a calculation
- Fetch data from an API
- Execute custom code

You define available tools, and the model decides when to use them based on the conversation. After a tool executes, you provide the result back to the model so it can continue the conversation.

This enables LLMs to go beyond pure text generation and interact with your application's functionality.

## Thinking/Reasoning

Some models have been specifically trained to reason through complex tasks step-by-step before giving a final answer. You can check the model's Hugging Face page to find out whether it supports thinking. 

Inside the LLM's raw output, the thinking/reasoning content usually appears between model-specific tags:

```
<think>
...reasoning steps...
</think>
...final answer...
```

Here are some tags examples:
- Qwen3: `<think>...</think>`
- Gemma 4: `<|channel>thought ...<channel|>`
- Ministral: `[THINK]…[/THINK]`

These tags are model and template specific, so always check the model's chat template if you need to parse out the thinking content yourself.
