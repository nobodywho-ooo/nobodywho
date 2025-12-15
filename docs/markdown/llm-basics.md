---
title: LLM Basics
description: Essential concepts for working with language models in NobodyWho
sidebar_title: LLM Basics
order: 2
---

In the following section we will provide a very brief introduction to the most essential concepts you need to know in order to use NobodyWho propably. The concepts covered are tokens, context, samplers and tools. 

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
Having a larger context allows for longer and more complex conversations, but it also slows down the response time, as the model has to process a more tokens each time it generates a response.

## Samplers

LLMs don't output text directly. Instead, they generate a probability distribution over all possible next tokens. A **sampler** decides which token to actually pick from this distribution.  This process is divided into to two parts:  Adjusting the distribution and Selecting the next token.

### Adjusting the Distribution


Different sampling strategies affect the model's output:
- **Temperature**: Higher values make output more creative/random, lower values make it more focused/deterministic
- **Top-k/Top-p**: Limit which tokens are considered, filtering out unlikely options
- **Greedy**: Always pick the most likely token (deterministic but sometimes repetitive)

NobodyWho provides sampler presets for common use cases like JSON generation.

## Tools

Tools (also called function calling) allow the LLM to request external actions. Instead of just generating text, the model can indicate it wants to:
- Search a database
- Perform a calculation
- Fetch data from an API
- Execute custom code

You define available tools, and the model decides when to use them based on the conversation. After a tool executes, you provide the result back to the model so it can continue the conversation.

This enables LLMs to go beyond pure text generation and interact with your application's functionality.
