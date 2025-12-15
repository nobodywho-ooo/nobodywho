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

LLMs don't output text directly. Instead, they generate a probability distribution over all possible next tokens. Since the model weigths are static after training, this means that the same input tokens always generate the same distribution. Depending on the use case however, there are many possible ways of choosing a next token from this distribution. This is configured using a **sampler**. A **sampler** splits the process of choosing a next token into two parts: Shiftingh the distribution and Sampling the distribution.

### Shifting the Distribution
Before sampling the distribution to get the next token, it is possible to adjust the distribution provided by the LLM to encourage certain behavior. Examples of these adjustments are:

- **Temperature**: Higher values make output more creative/random, lower values make it more focused/deterministic.
- **Top-k/Top-p**: Limit which tokens are considered, filtering out unlikely options
- **Penalties**: Lower the probalities of tokens already present in the context.

It is important to note that the steps in this part of the process can be chained. So it is possible to first apply a Temperature shift and then Top-k.


### Sampling the distribution
Once the distribution has been shifted the next step is to actually sample the distribution. This can also be done a few different ways:

- **Dist**: Sample the distribution randomly. 
- **Greedy**: Always pick the most likely token (deterministic but sometimes repetitive)
- **Mirostat**: Advanced sampling presented in this [article](https://arxiv.org/abs/1904.09751)

Since this part actually chooses the next token, these cannot be chained.


NobodyWho also supports more advanced ways of configuraing a sampler, like for example follow a JSON Schema.

## Tools

Tools (also called function calling) allow the LLM to request external actions. Instead of just generating text, the model can indicate it wants to:
- Search a database
- Perform a calculation
- Fetch data from an API
- Execute custom code

You define available tools, and the model decides when to use them based on the conversation. After a tool executes, you provide the result back to the model so it can continue the conversation.

This enables LLMs to go beyond pure text generation and interact with your application's functionality.
