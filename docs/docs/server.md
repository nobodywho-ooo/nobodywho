---
sidebar_position: 4
title: OpenAI-compatible server
---

# OpenAI-compatible server

NobodyWho provides an **experimental** local server that implements the OpenAI Chat Completions API. It loads one model and requires no API key.

:::note

`nobodywho-server` is experimental! Not to be used for any production/sensitive loads.

:::

## Setup (for a coding agent)

Start the server:

```bash
uvx \
  --from 'git+https://github.com/nobodywho-ooo/nobodywho.git#subdirectory=nobodywho/server' \
  nobodywho-server \
  --model hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf \
  --name qwen
```

The server listens on `http://127.0.0.1:8888` by default.

To use with, for example [Pi](https://pi.dev/), load the [Pi extension](https://gist.github.com/duarteocarmo/39a3f1148e2e33c27fddbbef2d971deb):

```bash
NOBODYWHO_BASE_URL=http://127.0.0.1:8888/v1 \
  pi -e 'git:https://gist.github.com/duarteocarmo/39a3f1148e2e33c27fddbbef2d971deb.git' \
  --model nobodywho/qwen
```

And start working.


## API

The API is very minimal for now:

- `GET /health`
- `GET /v1/models`
- `POST /v1/chat/completions`

```bash
curl http://127.0.0.1:8888/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "qwen",
    "messages": [{"role": "user", "content": "Say hello."}],
    "stream": true
  }'
```


## Limitations

- single model 
- sequential generations 
- no auth of any kind 
- no `required` tool choice (only `auto`, `none`, or a named function)

## References

- [OpenAI API reference](https://developers.openai.com/api/reference/overview)
- [OpenAI Chat Completions API](https://platform.openai.com/docs/api-reference/chat/create)
- [OpenAI streaming guide](https://platform.openai.com/docs/guides/streaming-responses?api-mode=chat)

## Future enhancements

- Expose GGUF metadata through the API
- Publish an official NobodyWho Pi extension instead of a gist
