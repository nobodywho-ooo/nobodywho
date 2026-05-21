# Limit 2 plan — WorkerChat + tools (RPC bridge)

Folded into commit 49591ad1 as a record of the next step. Reverted from
the commit immediately afterward (per your direction) — file kept in the
working tree as a reference; commit history is unchanged.

## What it solves

`new WorkerChat(model, { tools: [...] })` currently errors. `WorkerChat`
runs inference inside a Web Worker so the page stays responsive, but
JS function references can't survive `postMessage` (structured clone
throws `DataCloneError` on functions). The `tools` array is silently
broken on the worker-backed path.

This is the second of the two `v1 limitations` I documented in commit
`643d69f9`. Plan A (commit `49591ad1`) unlocked the *mechanism* — async
tool dispatch in core lets the worker's wasm yield to its JS event loop
while awaiting a Promise. That's exactly what makes the postMessage
round-trip viable. Now the JS-side RPC wiring needs to land.

## Architecture

```
   main thread                              worker thread
   ─────────────                            ─────────────
   tools: Map<name, jsFunction>             tools: Map<name, RpcStub>
   pendingRpc: Map<id, oneshot::Sender>

   new WorkerChat({tools:[…]})
     │
     ├─ stash callbacks in main-side map
     │
     └──postMessage 'create-chat'─────────▶ create-chat (metadata only:
                                              [{name, description, schema}])
                                              │
                                              └─ build Tool whose function
                                                 is an async RPC stub

   chat.ask('Weather?') ────postMessage────▶ ask
                                              │
                                              ▼
                                              Worker::ask running…
                                              model emits tool-call
                                              dispatch hits RpcStub.call(args):
                                                let (tx, rx) = oneshot::channel()
                                                let id = uuid()
                                                pendingRpc.insert(id, tx)
                                                ────────────────────────
   ◀────postMessage 'tool-rpc'───────────── postMessage({type:'tool-rpc',
                                                       id, name, args})
                                                rx.await
                                                                      ↑ yields here
   onmessage 'tool-rpc':                     (JS event loop ticks; the
     fn = tools.get(name)                    'tool-rpc-reply' onmessage
     result = await fn(args)                  handler will be the next
     postMessage({type:'tool-rpc-reply',     thing it processes)
                  id, result}) ─────────────▶ onmessage 'tool-rpc-reply':
                                                pendingRpc.remove(id)
                                                  .send(result)
                                                                      ↓ rx completes
                                                inference continues
                                              ─────────────────────
```

The key trick: **the worker's Worker::ask future suspends at the tool
dispatch site**, the JS event loop runs (because the wasm thread is
paused), the `tool-rpc-reply` message gets delivered to the worker's
`onmessage`, that handler resolves the oneshot, the future resumes.
All of which works because Plan A made `Tool::function` return a
future.

## Implementation plan

### 1. Tool metadata serialization

Tools are postMessage'd as `{ name, description, jsonSchema }` — all
structured-cloneable. No function ref in the payload. ~5 LoC.

### 2. Main-thread WorkerChat changes (`js/src/lib.rs`)

- `WorkerChat::new` (or `WorkerChat::create_chat` if there's a factory):
  - Iterate `options.tools` (if present), strip them out of the options
    payload going to the worker
  - For each tool, store its `callback: js_sys::Function` in a per-
    WorkerChat-instance `HashMap<String, js_sys::Function>`
  - Send tool metadata-only array via the existing `create-chat`
    postMessage
- Install a worker `onmessage` handler that demuxes by `data.type`:
  - `'tool-rpc'` → look up the function, invoke it (awaiting if it
    returns a Promise — same Promise-aware shape as `tool_from_tagged`),
    postMessage `'tool-rpc-reply'` back with the same id + result string
  - All existing message types route to the existing dispatcher
    unchanged

~80 LoC.

### 3. Worker-thread plumbing (`js/src/lib.rs`)

Worker side already has a dispatcher for incoming messages. New shapes:

- `'create-chat'` handler: when the incoming options include `tools:
  [...]` metadata, build `Tool::new_async` instances whose async closures
  are RPC stubs:
  ```rust
  Tool::new_async(name, description, schema, move |args| {
      let stub = stub.clone();
      async move { stub.call(args).await }
  })
  ```
- `RpcStub::call(args)`:
  - generate a uuid request id
  - register a `tokio::sync::oneshot::channel()` in a per-worker
    `HashMap<String, oneshot::Sender<String>>`
  - `web_sys::DedicatedWorkerGlobalScope::post_message(...)` with
    `{type:'tool-rpc', id, name, args}`
  - `.await` the oneshot receiver; return the resolved string

- Worker's existing `onmessage` handler gets a new arm for
  `'tool-rpc-reply'`: look up the oneshot sender by id, `.send(result)`,
  remove from map.

~60 LoC.

### 4. Worker `onmessage` registration

`runInWorker` (`js/src/lib.rs` around line 1075) already installs an
onmessage closure. Extend it to handle the new message types. Need to
hold the `HashMap<String, oneshot::Sender>` somewhere worker-local —
either thread_local!, a `Rc<RefCell<…>>` captured by the closure, or
attached to the per-worker state struct that already exists at lib.rs:34.

~10 LoC.

### 5. Error paths

- Tool not in main-thread map → main posts `tool-rpc-reply` with
  `{result: "ERROR: tool 'foo' not registered"}` so the model still
  gets something.
- Main-side callback throws → catch, post error string back.
- Worker's `post_message` fails (rare) → resolve the oneshot with an
  error string so the model doesn't hang.
- WorkerChat is dropped mid-RPC → the oneshot Sender is dropped, the
  receiver wakes with `RecvError`, the stub returns `"ERROR: chat
  dropped"`. Inference will abort cleanly at the next yield.

## Files touched

- `js/src/lib.rs` (~150 LoC across `WorkerChat`, `runInWorker`'s
  dispatcher, and helpers)
- `js/scripts/tool-smoke.mjs` — add a `WorkerChat({tools: [...]})`
  variant. Same sync + async callback shapes as today's smoke;
  asserts callback invocation count.

## Out of scope for this slice

- **Promise-bearing tool callbacks** crossing into the worker get
  awaited on the main thread (where they live anyway) before the
  `tool-rpc-reply` postMessage. So async callbacks "just work" here,
  same shape as Plan A's in-process Chat path.
- **Bidirectional streaming tool output** (tool emits multiple
  partial results) — not needed for v1.
- **Tool-call cancellation from the worker side** — if the user
  aborts inference, in-flight tool RPCs just resolve to a generic
  cancellation string and the model never sees them. Adequate.

## Verification

1. Existing in-process `tool-smoke.mjs` paths keep passing (no
   regression from added worker plumbing).
2. New WorkerChat-tools smoke section: sync callback, then async
   callback. Same Qwen3-0.6B model.
3. Round-trip the same weather example through WorkerChat. Assert
   the callback was invoked, the final response contains the tool's
   output substring.

## Estimated effort

~150 LoC total, one commit. No core changes. ~1-2h once you start.
