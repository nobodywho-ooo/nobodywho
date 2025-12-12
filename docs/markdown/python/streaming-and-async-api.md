---
title: Streaming & Async API
description: NobodyWho is a lightweight, open-source AI engine for local LLM inference. Simple, privacy oriented with no infrastructure needed.
sidebar_title: Streaming & Async API
order: 3
---

Synchronously waiting for the full response to arrive can be costly. If your application
domain allows you to, you would ideally want to stream the tokens to the user as soon
as they arrive, and spend the time in between doing useful work, rather than just waiting.

## Streaming tokens

Allowing streaming is super simple. Instead of calling the `.completed()` method, just iterate
over the response object:
```python
chat = Chat('./model.gguf')
response = chat.ask('How are you?')
for token in response:
    print(token)
```

Still, bear in mind that for the individual tokens, you are waiting synchronously.

## Async API

If you don't want to wait synchronously, swap out the `Chat` object for a `ChatAsync`. All of the API stays the same, so either you can opt for a full completed message:
```python
import asyncio
from nobodywho import ChatAsync

async def main():
    chat = ChatAsync('./model.gguf')
    response = await chat.ask('How are you?').completed()
    print(response)

asyncio.run(main())
```
Or again stream tokens:
```python
import asyncio
from nobodywho import ChatAsync

async def main():
    chat = ChatAsync('./model.gguf')
    response = chat.ask('How are you?')
    async for token in response:
        print(token)

asyncio.run(main())
```

Similarly, the other model types we support also implement async behaviour, so
you can go for `EncoderAsync` and `CrossEncoderAsync`, which are
both part of the [embeddings & rag functionality](./embeddings-and-rag.md).
