# Recorded Responses API streams

Fixtures for testing the Rust event-stream implementation, recorded against
live APIs with the OpenAI Python SDK.

Each file is a flat JSON list of the raw SSE `data:` payloads of one
conversation, in arrival order, concatenated across every response in that
conversation. `sequence_number` restarts at 0 for each response, so split on
`response.created` to get the individual response streams.

## Targets

The same six conversations are recorded against each provider, so the files can
be compared directly. The Rust tests replay every directory.

| Directory | Provider | Model |
| --- | --- | --- |
| `.` | OpenAI | `gpt-5-nano` |
| `openrouter/gpt-5-nano/` | OpenRouter | `openai/gpt-5-nano` |
| `openrouter/claude-haiku-4.5/` | OpenRouter | `anthropic/claude-haiku-4.5` |

## Conversations

| File | Contents |
| --- | --- |
| `simple_text.json` | One plain text response. |
| `multi_turn_text.json` | Three user turns, text only. |
| `single_tool_call.json` | One `get_weather` call, then the answer built on the tool result. |
| `parallel_tool_calls.json` | Three `get_weather` calls in one response, then the answer. |
| `reasoning_with_tool_call.json` | Reasoning enabled, tool calls, then the answer. |
| `incomplete_max_output_tokens.json` | Truncated by `max_output_tokens`; ends in `response.incomplete`. |

`mcp/mcp_calls.json` is recorded against OpenAI alone (`gpt-6-luna`), as the
only target that runs MCP servers: calls OpenAI makes itself on
[DeepWiki](https://mcp.deepwiki.com/mcp), one of which fails on a repository
that doesn't exist, then a follow-up turn that doesn't list the tools again.

Tool calls were answered with a canned result,
`{"temperature_c": 17, "conditions": "cloudy"}`. Reasoning is requested as
`{"effort": "low", "summary": "auto"}`, except for Claude, which has no
reasoning summaries and gets `{"effort": "low"}`.
