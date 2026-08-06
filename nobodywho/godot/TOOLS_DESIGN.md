# Godot bindings — tool calling design

Status: **design document**. Authoritative for Phase 3 of the rewrite
(`REWRITE_PLAN.md` §8). Companions: `REWRITE_PLAN.md`, `ASYNC_RESEARCH.md`,
`STATE_MANAGEMENT_RESEARCH.md`, `REWRITE_ROUGH_EDGES.md` §C.

This document is written to be readable on its own: it explains *why* each
choice is what it is, with the supporting research, so a newcomer can follow
the design process — not just read the conclusion.

## 1. What tool calling is

Tool calling lets the model invoke functions during generation. The flow,
driven by core (`core/src/chat.rs:1805`):

1. The worker generates a response under a grammar that constrains output to
   valid tool-call JSON.
2. The tool-format handler extracts `Vec<ToolCall>` (each a `{name, arguments}`).
3. For each call, the worker looks up the matching `Tool` and invokes
   `(tool.function)(tool_call.arguments)` — a `Fn(serde_json::Value) -> String`.
4. The string result is appended to history as a `Tool` message.
5. The worker generates again. Loop until the model emits no more tool calls.

The entire loop runs **inside the worker thread**, synchronously. Step 3 is
the only seam where the binding gets involved: core calls *our* closure, and
doesn't resume until we return a `String`.

The core `Tool` type (`core/src/tool_calling/mod.rs:39`):

```rust
pub struct Tool {
    pub name: String,
    pub description: String,
    pub json_schema: serde_json::Value,
    pub function: Arc<dyn Fn(serde_json::Value) -> String + Send + Sync>,
}
```

`function` is **sync**, returns a **`String`**, and runs on the **worker
thread**. That last point is the root of every design decision below.

Core also ships two built-in tools whose `function` is pure Rust —
`Tool::python(max_duration, max_memory, max_recursion_depth)` and
`Tool::bash(max_commands)` — so they need **no** GDScript marshalling.

## 2. The two problems this phase must solve

### Problem A — GDScript callables must run on the main thread

The old bindings called `callable.call(&args)` *from the worker thread* under
`unsafe impl Send for SendCallable` (`REWRITE_ROUGH_EDGES.md` §C.10). That is
unsound: it touches the scene tree, other nodes, and the GDScript VM off the
main thread. It "works" only because `experimental-threads` is on.

The rewrite marshals every GDScript tool call to the **main thread**. The
worker-side `Fn` closure packages the model's raw JSON arguments into a
request (the `Callable` itself never crosses threads — it's `!Send`, see
§5), ships it to that tool's main-thread loop, and **blocks the worker** on
a `std::sync::mpsc` recv until the main thread produces the result string. Blocking the *worker* is fine — it's a std thread, not the render
thread; the main thread stays free to run the callable and pump frames. The
old "never block the main thread" rule is about the *render* thread; the
worker blocking on a tool result is exactly what core already does
synchronously in step 3.

### Problem B — async tools (GDScript coroutines)

The plan explicitly wants tools that `await` — play an animation, query a
node next frame, wait on a timer. When a GDScript function containing `await`
is called via `Callable::call`, it returns a `GDScriptFunctionState` (a
suspended coroutine) and completes later as frames pump, emitting its
`completed` signal with the return value.

The old bindings *detected* this case and rejected it:

```rust
// old code, main:nobodywho/godot/src/lib.rs:1434
if res.get_type() == VariantType::OBJECT {
    if let Ok(obj) = res.try_to::<Gd<RefCounted>>() {
        let class_name = obj.get_class();
        if class_name.to_string() == "GDScriptFunctionState" {
            godot_error!("Tool function is async. This is not supported yet.");
            return "Error: Async tool functions are not supported. ...".into();
        }
    }
}
res.to_string()
```

Phase 3 must instead **await** that coroutine: recognize the
`GDScriptFunctionState` return, connect to its `completed` signal, convert
that signal to a Rust future, await it inside the main-thread runner, and
only then send the resulting `String` back to the worker. This is the
"await a GDScript coroutine from Rust" case the plan §C calls the central
design item, and the part most likely to have a rough edge — hence the
research spike in §9.

## 3. The re-entrancy deadlock

While a tool runs on the main thread, **the worker is blocked** awaiting the
tool's result (step 3 above). If the tool's GDScript calls back into the
*same* chat — `chat.ask(...)`, `chat.get_chat_history()`,
`chat.set_system_prompt(...)`, anything that sends a `ChatMsg` — that
message lands in the worker's `mpsc` channel, but the worker can't process
it (it's parked inside `tool.function`). The tool never completes → the
worker never unblocks → **deadlock**.

Two clarifications for the user docs:
- This is a *hang*, not an engine freeze. The main thread keeps pumping
  frames; what hangs forever is that chat's generation and any GDScript
  coroutine awaiting it.
- Calling into a **different** chat from a tool is fine — the hazard is
  strictly the chat whose worker is blocked on this tool.

### Decision: detect-and-error guard, included in Phase 3

(Supersedes the earlier "document-as-forbidden now, guard later" decision.
The per-tool-loop design of §5 makes the guard nearly free, and a silent
forever-hang is the single worst failure mode this rewrite exists to
eliminate — deferring it saved almost nothing.)

The guard: `NobodyWhoChat` owns an `Arc<AtomicBool>` "tool in flight" flag.
At registration time (§5), each of that chat's worker-side `Fn` closures
captures a clone and sets it around the blocking `recv()` (set before the
request is sent, cleared after the result arrives). Every chat `&self`
method checks the flag first and, if set, resolves `null` +
`godot_error!("called back into this chat from one of its own tools — the
worker is blocked waiting for the tool to return; use a different chat, or
return first")`. That's ~10 lines on top of the design as written: the flag
is owned per-chat, threaded into the closures at the one place they're
built, and checked at the one place methods enter.

No ordering subtleties: the flag is only ever set while the worker is
parked inside `tool.function`, and chat methods run on the main thread —
the same thread the tool's GDScript runs on — so a re-entrant call always
observes the flag set. (Relaxed ordering suffices; it's a same-thread
read in the case that matters.)

The user-facing docs still carry the rule — *"Do not call back into the
same `NobodyWhoChat` from inside one of its tools"* — but a violation now
fails loudly with an actionable error instead of hanging.

## 4. JSON-schema generation from GDScript type hints

### What the old bindings did (and the docs confirm is possible)

The old `json_schema_from_callable` (`main:nobodywho/godot/src/lib.rs:2102`)
reflected on the callable via Godot's method metadata:

```rust
let method_name = callable.method_name().ok_or("...")?;
let method_obj  = callable.object().ok_or("...")?;
let method_info = method_obj.get_method_list().iter_shared()
    .find(|d| d.at("name").to::<String>() == method_name.to_string())?;
let method_args: Array<VarDictionary> = method_info.at("args").to();

for arg in method_args.iter_shared() {
    let arg_name: String = arg.at("name").to();
    let arg_type: VariantType = arg.at("type").to();
    let json_type = match arg_type {
        NIL => return Err("arguments must all have type hints..."),
        BOOL => "boolean", INT => "integer", FLOAT => "number",
        STRING => "string", ARRAY => "array",
        _ => return Err("Unsupported type..."),
    };
    properties.insert(arg_name, json!({ "type": json_type }));
    required.push(arg_name);
}
```

So **Godot absolutely exposes per-argument type hints** through
`Object::get_method_list()` → each arg is a `{name, type}` dict where `type`
is a `VariantType`. gdext 0.5 surfaces this as `Vec<MethodInfo>` with
`MethodInfo::arguments: Vec<PropertyInfo>` (`PropertyInfo` carries
`name` + `variant_type`). The mechanism is unchanged; the old code's
"not super clean" reputation comes from the linear-scan `find` and the
`VarDictionary` digging, not from a missing API.

**Limitations of the old generator (and which are choices vs. platform gaps):**
- No per-argument *description* (GDScript has no syntax for it — a real
  platform gap). The user passes a single `description` for the whole tool;
  arg descriptions stay empty unless they use the manual-schema path.
- All arguments are `required`. **This is a v1 choice, not a platform
  limitation**: `get_method_list()` dicts carry a `default_args` array, so
  trailing args with defaults could be marked non-required and filled with
  the default when the model omits them. Deferred, not impossible — revisit
  if users ask.
- Only primitive JSON types: `boolean`, `integer`, `number`, `string`,
  `array`. No nested objects, no enums, no `oneOf`. Users who need those
  pass a manual schema. (Partial lift available later: typed arrays like
  `Array[String]` surface through `hint`/`hint_string` in the property
  info, so `{"type": "array", "items": {...}}` is derivable. Also v1-deferred.)
- Anonymous functions / static methods are rejected **on the auto path**
  (`callable.object()` is `None`, so there's nothing to reflect on); the
  callable must be a bound method on an Object. This is fine for the
  `extends Node` / `extends RefCounted` GDScript pattern — and the manual
  path deliberately does *not* inherit this restriction (below).

### Decision: auto-generate from type hints, with a manual-schema escape hatch

Phase 3 keeps **both** paths, mirroring the old `add_tool` /
`add_tool_with_schema` split and the Python `@tool` / manual-schema split:

- **`NobodyWhoTool.create(callable, description)`** — auto-generates the
  JSON schema from the callable's argument type hints (the mechanism above,
  ported to gdext 0.5 `PropertyInfo`). The tool's **name** is
  `callable.method_name()` — which exists by construction, since the auto
  path requires a bound method. The common, ergonomic path.
- **`NobodyWhoTool.create_with_schema(name, description, json_schema, callable)`**
  — `json_schema` is a `Variant` (a Dictionary, or a JSON string parsed to
  a `serde_json::Value`). For when the user needs enums, nested objects,
  arg descriptions, or optional fields. The tool name is an **explicit
  parameter** here: the manual path needs no reflection at all (`callv`
  works on any callable), so it accepts lambdas and unbound callables —
  making it a true escape hatch, not just "auto minus the schema".

**Argument-ordering contract** (how the model's JSON object becomes
positional args — see §5): when the callable is a bound method, argument
order is taken from the **method info** (authoritative — it's the real
declaration order), and the schema's `properties` are treated as a lookup
that must cover the same names. Only for unbound callables (lambdas, where
no method info exists) does the order fall back to the schema's `properties`
insertion order — which is preserved (Godot `Dictionary` preserves insertion
order; the Godot crate enables serde_json's `preserve_order` feature for the
JSON-string path — that feature is load-bearing, keep it). Document the
fallback rule loudly on `create_with_schema`.

Both factory variants are static `#[func]`s (gdext classes can't take `.new()`
args), returning a `Gd<NobodyWhoTool>`.

### Why not a GDScript DSL

A helper DSL ("build a schema with `Schema.object({...})`") would be pure
syntactic sugar over passing a Dictionary. It's not needed for v1; a literal
Dictionary is honest and matches the docs. Can be added later if users find
themselves repeating boilerplate.

## 5. The main-thread tool runner

### The constraint that shapes everything: `Callable` is `!Send`

Before choosing an architecture, one fact from the gdext 0.5.4 source
(verified; `godot-core-0.5.4/src/builtin/`): **`Callable` and `Variant` are
`!Send`, even with `experimental-threads`** — only `GString`/`StringName`
carry an `unsafe impl Send`. And `godot::task::spawn` asserts it is called
on the main thread (`godot-core-0.5.4/src/task/async_runtime.rs:156`).
Consequences:

1. The core `Tool::function` closure must be `Send + Sync`
   (`core/src/tool_calling/mod.rs:43`), so it **cannot capture the GDScript
   `Callable`** — the callable must live its entire life on the main thread.
   Only `Send` data may cross to the worker: `serde_json::Value`, `String`,
   channel handles. (The old bindings' `unsafe impl Send for SendCallable`
   was exactly the workaround for this; it's banned.)
2. JSON→Variant argument conversion must happen **on the main thread** (a
   `Vec<Variant>` can't be sent over a channel).
3. The worker cannot initiate anything on the main thread directly: it can't
   `spawn` (main-thread assert), can't `call_deferred` (needs a
   `Callable`/`Gd`, both `!Send`). The **only** worker→main mechanism is a
   channel with a persistent main-thread receiver. This rules out a "pure
   per-request" design (spawn a fresh task per tool call, nothing
   persistent) — it is impossible without reintroducing `unsafe Send`.

### Decision: one small `godot::task::spawn` loop **per registered tool**

At registration time — inside the `set_tools`/`reset_chat`/`create` task,
which runs on the main thread — each GDScript-backed tool gets its own tiny
spawned loop that *owns* that tool's `Callable` (it never crosses threads),
plus the captured `properties` order for argument mapping. The core `Tool`
closure captures only the loop's `UnboundedSender` (which is `Send`):

```rust
// main thread, per registered tool, at set_tools/create time
let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ToolRequest>();

struct ToolRequest {
    args_json: serde_json::Value,                // Send: raw model output
    result_tx: std::sync::mpsc::Sender<String>,  // Send: worker blocks on the pair's rx
}

godot::task::spawn(async move {
    // this future owns: callable (Callable), properties (Vec<String>)
    while let Some(req) = rx.recv().await {
        let args = map_json_to_args(&req.args_json, &properties); // main-thread conversion
        let res: Variant = callable.callv(&args);
        let value = match try_as_coroutine(&res) {
            // async tool: await the GDScriptFunctionState.completed signal
            Some(state) => await_coroutine(state).await,
            None => res,
        };
        let _ = req.result_tx.send(stringify(value));
    }
    // rx returned None: core dropped the Tool closure (set_tools replaced it,
    // or the chat was freed) → loop ends, callable released. Self-cleaning.
});

// worker-side closure, installed as Tool::function
// (Send + Sync: captures only tx, plus the chat's re-entrancy flag — §3)
let func = move |j: serde_json::Value| -> String {
    let (result_tx, result_rx) = std::sync::mpsc::channel();
    if tx.send(ToolRequest { args_json: j, result_tx }).is_err() {
        return "Error: tool runner gone".into();
    }
    // recv_timeout, not recv: see "Failure hardening" below (§5)
    match result_rx.recv_timeout(timeout) {
        Ok(r) => r,
        Err(RecvTimeoutError::Timeout) => format!("Error: tool '{name}' timed out"),
        Err(RecvTimeoutError::Disconnected) => "Error: tool runner gone".into(),
    }
};
```

Channel types are pinned by the `!Send` analysis, not open questions:
worker→main is `tokio::sync::mpsc::unbounded_channel` (worker-side `send` is
synchronous and never blocks, no tokio runtime needed; main-side
`recv().await` works on the gdext executor via the cross-thread waker path
already vetted in `REWRITE_PLAN.md` §9). Main→worker is a plain
`std::sync::mpsc` pair per request: the worker blocks on `recv()` (it's a
std thread with nothing else to do) and the main thread sends synchronously.
`std::sync::mpsc::Receiver::recv()` in a spawned future is forbidden — it
would block the main thread.

**Worker blocking is intentional and correct.** Core's tool loop is already
synchronous (§1 step 3); the worker has nothing else to do while a tool
runs. The main thread is never blocked — it runs each tool loop
cooperatively with everything else.

### Why per-tool loops, and not the alternatives

Three architectures were compared (a global runner, a per-chat runner with a
callable registry, and per-tool loops). Per-tool wins on every axis that
matters; the reasoning in full, because it's the load-bearing decision of
this phase:

**Rejected: one global runner for all chats.** An earlier draft of this
document proposed a single spawned loop started once, serving every chat.
Problems: it needs global state (a `OnceLock` sender, lazy init), it has an
unanswered lifecycle question (what happens on extension deinit / editor
reload while the loop holds `Callable`s?), and it serializes tool execution
*across* chats — an async tool awaiting a 10-second timer in chat A stalls
chat B's tools, even though core only requires serialization *per worker*.
And because the channel can't carry `Callable`s (point 1 above), it would
need the same registry machinery as the per-chat option below, centralized.

**Rejected: one runner per chat, with a callable registry.** Since the loop
can't receive `Callable`s over the channel, a per-chat runner needs a
main-thread-owned registry — `Rc<RefCell<HashMap<ToolId, Callable>>>` shared
between the `NobodyWhoChat` and its loop future — and the worker sends
`{tool_id, args_json, result_tx}`. That works, but tool mutation
mid-conversation gets ugly: `set_tools` must (a) insert new registry
entries, (b) build new core `Tool`s capturing new ids, (c) await
`handle.set_tools(...)`, (d) purge old entries — but only once core has
actually dropped the old closures, or an in-flight generation could call a
purged id. Two synchronized views of the tool set (registry + core worker
state) with an ordering hazard between them — exactly the class of
two-places-for-one-fact bug the rewrite exists to eliminate
(`REWRITE_ROUGH_EDGES.md` §B.7). Forgetting to purge also leaks strong
refs: a `Callable` bound to a `RefCounted` keeps it alive, so stale registry
entries pin user objects indefinitely.

**Partially adopted: per-request execution *inside* the per-tool loop.**
"Pure" per-request is impossible (point 3 above). An earlier revision of
this document rejected the realizable version — a persistent dispatcher
that spawns a sub-task per request — on the grounds that it only buys
concurrency between tool calls, which is unreachable (core executes tool
calls strictly serially within a generation, §1). That argument was right
about concurrency but missed the real benefit: **fault isolation**. With
inline execution, an async tool whose coroutine never completes (awaiting a
signal that never fires) parks the loop *forever* — the worker's
`recv_timeout` unblocks generation, but every later call to that tool
queues behind the wedged await and times out: the tool is silently dead for
the rest of the session. (The freed-state error branch can't save it either:
the loop itself holds a `Gd` clone of the coroutine state, so it is never
freed.) The implementation therefore keeps the per-tool loop as the
`Callable`-owning **dispatcher**, but each request runs in its own spawned
sub-task (legal: the dispatcher runs on the main thread, where `spawn` is
allowed). A wedged coroutine wedges only its own call; the timeout becomes
genuinely recoverable; and the timeout measures execution rather than
queue-wait. Verified by the timeout-recovery test: first call wedges, the
second call to the same tool runs and succeeds.

**Chosen: per-tool-registration loops.** The decisive property is that the
**loop's lifetime is mechanically derived from core's ownership of the
`Tool` closure**, via sender-drop:

- The core `Tool` closure is the *only* holder of the loop's sender. When
  `set_tools`/`reset_chat` replaces the worker's tool vec, core drops the
  old closures → senders drop → `recv()` yields `None` → old loops end
  themselves → `Callable`s released. No purge step, no id bookkeeping, no
  "when is it safe to remove" question, no leaked user objects.
- New tools = new loops, spawned in the `set_tools` task (main thread)
  *before* `handle.set_tools(...)` is awaited — so there is no window where
  a core-registered tool has no live receiver.
- The in-flight edge case is safe for free: if a generation is mid-tool-call
  when a `set_tools` is queued, the old closure (and therefore its loop)
  stays alive exactly as long as core holds it. The two lifetimes cannot
  desynchronize, because one *is* the other.
- Chat teardown is the same story: chat freed → handle dropped → worker
  exits → closures dropped → loops end. No `on_stage_deinit` hook, no
  global state, nothing to forget.
- The same `NobodyWhoTool` object added to two chats, or re-added after
  removal, just produces independent registrations — fresh core `Tool` +
  fresh loop each time, nothing shared, nothing to invalidate.
- Isolation matches core's actual requirement: tools within one chat are
  serialized (by core's own loop — a per-tool loop is never contended,
  because core calls one tool at a time per worker), while chats never wait
  on each other.

Cost: one parked future per (tool × chat) registration in gdext's task
list. Negligible — a suspended future waiting on an empty channel does no
work.

This is also consistent with the rest of the rewrite: no new `Node`, no
`_process`, no global state — the registration *is* the state, and dropping
it is the cleanup.

### Argument marshalling

The model emits JSON; the callable wants positional GDScript args. Flow:
1. Worker receives `serde_json::Value` (an object) and sends it to the
   tool's loop **unconverted** (it's the only `Send` representation).
2. **Main-thread side** (inside the loop) maps it to `Vec<Variant>` in
   *argument-declaration order* (the `properties` key list captured at
   registration — same trick the old code used to preserve order). Uses
   `json_to_variant` from `convert.rs` (already implemented in Phase 2).
3. The loop calls `callable.callv(&args)`.

Result marshalling: `tool.function` returns `String`. A `String` return
passes through as-is (the documented contract: tool functions should be
`-> String`). Non-string returns are **JSON-encoded** via a
`variant_to_json` inverse of `json_to_variant` in `convert.rs` — not
`res.to_string()`: Godot's `str()` repr of a Dictionary/Array is close to
but not JSON, and models are trained on JSON tool results. For async tools,
the coroutine's `completed` payload is handled the same way.

### Failure hardening (all resolve to an error *string* for the model)

Tools run in games; their targets get freed and their coroutines die.
Every failure mode below returns an error string to the model (which can
recover) rather than hanging or crashing:

- **Freed callable.** Tools are typically bound to `Node`s, and the node
  can be freed between registration and the model deciding to call the
  tool. The loop checks `callable.is_valid()` before `callv`; if invalid,
  it returns `"Error: tool '<name>' is no longer valid (its object was
  freed)"`.
- **Script error / bad call.** A `callv` that hits a script error returns
  nil (Godot logs the error itself); the loop returns an error string
  rather than JSON-encoding `null` as a fake success.
- **Coroutine that never completes.** An async tool awaiting a signal that
  never fires — or a coroutine that dies on a script error mid-await, so
  `completed` never emits — would otherwise wedge the worker forever, and
  `stop_generation()` can't reach it (the worker is parked in `recv()`,
  not generating). Two layers handle this: the worker-side closure uses
  `recv_timeout(TOOL_TIMEOUT)` with a generous default (60s; per-tool
  override via an optional `timeout_secs` on the factories) and returns
  `"Error: tool '<name>' timed out"` on expiry, and the dispatcher runs
  each request in its own sub-task so the wedged await doesn't poison the
  tool for later calls (see "Partially adopted: per-request execution"
  above). A timed-out tool that *later* completes sends its result into a
  dropped channel — harmless.
- **`Callable.bind()` pre-filled arguments.** Godot appends bound args
  after the caller-provided ones (they fill the declaration from the
  right), so schema reflection excludes the last
  `get_bound_arguments_count()` declared args — otherwise the model would
  supply them too and every call would be an arg-count script error.
- **Invalid tool names.** Names flow into the tool-call GBNF grammar and
  the chat template; `create_with_schema` validates identifier shape
  (`[a-zA-Z_][a-zA-Z0-9_]*`) at construction, where the error is
  actionable, instead of failing at ask-time inside grammar generation.
  (The auto path is safe by construction — method names are identifiers.)
- **Freed `GDScriptFunctionState`.** Awaiting the `completed` signal must
  use `FallibleSignalFuture` (`to_fallible_future()`), **never**
  `SignalFuture`/`to_future()` — the latter panics if the signal object is
  freed, and a panic inside a `godot::task` future is swallowed by gdext
  and hangs everything silently (the plan's own never-panic rule). The
  fallible branch resolves to an error string. This is a requirement, not
  a spike question (§6).

### Resolved false alarm: the "gdext 0.5.4 Callable factory bug"

During implementation, the GDScript-tool factories were briefly blocked on
what looked like a gdext bug: the factories panicked in
`FromGodot::from_variant` with *"expected array of type Untyped, got
Builtin(DICTIONARY)"*, and the failure was attributed to "a `#[func]` with
a `Callable` arg returning `Gd<NobodyWhoTool>`". **That diagnosis was
wrong.** The real cause, confirmed from primary sources:

- Godot returns each method-info dict's `args` field as a **typed**
  `Array[Dictionary]` (`core/object/object.cpp`: `MethodInfo::to_dict()`
  → `d["args"] = convert_property_list(...)`, which returns
  `TypedArray<Dictionary>`).
- gdext 0.5 **strictly refuses** to convert a typed array into
  `Array<Variant>` (`array.rs` `with_checked_type`: `Untyped` is not
  compatible with `Builtin(DICTIONARY)`) — producing exactly that error.
- The panic site was our own schema reflection:
  `method_info.get_or_nil("args").to::<VarArray>()`. Every factory shape
  "reproduced" it because every shape ran schema reflection on a real
  Callable; the `Gd<NobodyWhoSamplerConfig>` and `python()` controls
  "worked" because they never reached that line. The Callable parameter
  and the `Gd<NobodyWhoTool>` return were never involved.

**Fix:** use the correctly-typed `Array<VarDictionary>` for `args` (as the
old bindings did). Additionally, `variant_to_json`'s `ARRAY` branch had the
same latent panic for *any* typed array (a tool returning `Array[String]`
would panic inside a `godot::task` future → silent hang); fixed by using
gdext 0.5's type-erased `AnyArray`, which accepts typed and untyped arrays
alike. Both factories are enabled and the full path — sync tool, async
tool (coroutine awaited via `completed`), auto-schema — passes the
model-backed test suite.

**Rule to carry forward:** any conversion of an engine-supplied array
must either name the exact element type (`Array<VarDictionary>`) or use
`AnyArray` — never `.to::<VarArray>()`, which panics on typed arrays.

## 6. Async-tool detection

This is the part the project owner explicitly flagged as needing diligent
research before selecting an option. The research converged on **Approach 1**;
§9's empirical spike has now **confirmed** it (results below). The decision
is locked.

### What is known

- Calling a GDScript function that contains `await` via `Callable::call`
  returns a `GDScriptFunctionState` (a `RefCounted` subclass) instead of the
  final value. The coroutine resumes on subsequent frames and emits its
  `completed` signal with the return value when done.
- gdext 0.5.4 has `Signal::to_future::<R>()` and `TypedSignal::to_future()`
  (`godot-core-0.5.4/src/task/futures.rs:361,379`), backed by
  `SignalFuture`/`FallibleSignalFuture`. These are designed for exactly
  "await a Godot signal from a Rust future on the gdext executor." The old
  code already used `tree.signals().process_frame().to_future().await` in
  its `wait_for_*_signal_connect` helpers, so the primitive works.
- The old code's detection was: `res.get_type() == VariantType::OBJECT` →
  `try_to::<Gd<RefCounted>>()` → `get_class() == "GDScriptFunctionState"`.

### Candidate approaches

**Approach 1 — detect `GDScriptFunctionState` by class name, await its
`completed` signal.** The direct port of the old detection plus the await
that was missing. Get the `completed` signal with
`Signal::from_object_signal(&state, "completed")`, call
`.to_fallible_future::<(Variant,)>()` (fallible — required, see §5's failure
hardening; the `(Variant,)` is because `completed` emits one arg, the
return value), await it in the loop, marshal the payload.

**Approach 2 — duck-type on the `completed` signal.** Instead of matching a
class name, check whether the returned object has a `completed` signal
(`Object::get_signal_list` / `find_signal`). More robust to engine internals
renaming the class — but *less* precise: any object a tool legitimately
returns that happens to carry a `completed` signal would be silently
mis-awaited instead of marshalled as a value.

**Approach 3 — always `await` the return as a signal-or-value.** Treat every
tool return as "maybe async". A false economy: the sync/async branch exists
either way, only the predicate differs — and it inherits Approach 2's
mis-await risk.

**Approach 4 — require the user to declare async tools.** A separate
`NobodyWhoTool.create_async(...)` that always awaits. Avoids detection
entirely but is worse ergonomics and easy to get wrong (forget the
`_async`, hang). Not favored.

### Decision (confirmed by spike): Approach 1

The class name `GDScriptFunctionState` has been stable since Godot 4.0,
the check is precise (no mis-await risk), and it's the same predicate the
old bindings shipped for years. Confirmed shape: class-name string compare
(the class is not in gdext's generated list — it's GDScript-internal, but
`get_class()` returns the ClassDB name so the string compare works) +
hold a `Gd` clone of the state across the await + `FallibleSignalFuture`
typed as `(Variant,)` — `completed` emits one arg (the return value).

**Status: confirmed end-to-end.** The §9 spike verified the mechanism, and
the model-backed test suite exercises the full path: sync GDScript tool
(auto-schema, correct arg, result reaches the model) and async GDScript
tool (coroutine suspends on a timer, `completed` awaited from the per-tool
loop, return value reaches the model's final answer). The factory panic
that briefly blocked this path was a misdiagnosis — see §5 "Resolved false
alarm".

Known limitation this bakes in, to document: only **GDScript** coroutines
are detected. A C# `async` method invoked via `Callable` returns a .NET
`Task`, not a function state — C# tools must be synchronous (or GDScript
wrappers). Fine for v1.

### What §9 verified (results recorded)

The spike confirmed Approach 1 on every point: (a) the fallible
signal-future on `GDScriptFunctionState.completed` resolves, on the gdext
executor, from inside a `godot::task::spawn` future, with the resolved
payload **equal to the coroutine's return value** (not empty); (b) the
class is reachable by `get_class()` string compare (`"GDScriptFunctionState"`);
(c) holding a `Gd` clone keeps the state alive for the duration of the
await. The freed-state error branch was not exercised by the spike but is
required by §5's failure hardening (use `to_fallible_future`, never
`to_future`). See §9 for the harness and the one gotcha that cost a
round-trip.

## 7. API surface

### `NobodyWhoTool` (`src/tools.rs`, new)

```gdscript
# Auto-schema from type hints (the common path):
var t = NobodyWhoTool.create(get_player_stats, "Returns the player's health, mana, gold.")

# Manual schema (enums, nested objects, arg descriptions; works with lambdas):
var t = NobodyWhoTool.create_with_schema(
    "press_button", "Press a colored button (red/blue/green).",
    press_button_schema, press_button)

# Built-in pure-Rust tools (no GDScript callable, bypass the main-thread runner):
var t = NobodyWhoTool.python()        # optional: max_duration_secs, max_memory_bytes, max_recursion_depth
var t = NobodyWhoTool.bash()          # optional: max_commands
```

Note for the user docs: `python()` and `bash()` are **sandboxed
interpreters** built into core (a Rust Python interpreter with resource
limits, and an in-memory bash with no host filesystem/network/env access) —
not host shell execution. The names will scare users otherwise; say this
prominently. It also means they work identically on all platforms,
including Windows.

`NobodyWhoTool` is `#[class(no_init, base=RefCounted)]` — built only via the
static factories. Internally holds either:
- a fully-built `core::tool_calling::Tool` (for `python`/`bash` — the
  `function` is core's pure-Rust closure, no marshalling), or
- a `name` + `Callable` + `description` + `json_schema` +
  `properties: Vec<String>` + `timeout` (for GDScript tools — assembled
  into a core `Tool` at `set_tools`/`create` time, which is also when the
  tool's main-thread loop is spawned and the worker-side `Fn` closure
  capturing its sender and the chat's re-entrancy flag is built — §3, §5).

The split exists so the built-in tools never spawn a loop at all.

### `NobodyWhoChat` changes (`src/chat.rs`)

- `create(model, config)` gains a `"tools"` key: `Array` of
  `Gd<NobodyWhoTool>`, plumbed into `ChatConfig.tools`.
- **`set_tools(tools: Array)`** — the Phase-2-deferred method, now lands.
  `&self`, async via `task()`, resolves to `null` on success / `null` +
  `godot_error!` on failure (minimal convention).
- **`reset_chat(system_prompt: Variant, tools: Array)`** — the other
  Phase-2-deferred method. `system_prompt` is a String or null (clear);
  `tools` is an Array of `Gd<NobodyWhoTool>`. Same `&self`/`task()` shape.

### What stays as-is

- `ask` / `stop_generation` / all Phase 2 query-mutation methods — unchanged
  in signature; each gains the §3 re-entrancy-guard check at entry.
- `NobodyWhoSamplerConfig` etc. — unchanged.
- The minimal `null`-on-error convention — unchanged. A tool that errors
  returns an error *string* to the model (so the model can recover), not a
  `null` to GDScript; the `tool.function` closure always returns a `String`.

## 8. Module layout & implementation order

```
src/
  lib.rs        # mod tools;
  tools.rs      # NobodyWhoTool, the per-tool main-thread loop, the worker-side Fn bridge
  chat.rs       # "tools" config key on create; set_tools; reset_chat
  convert.rs    # json_to_variant (already present) — used for arg marshalling
tests/
  tools_spike_test.gd   # §9 research spike (throwaway)
  tools_test.gd         # real suite: sync tool, async tool, built-in python/bash
```

**Implementation order (each step leaves the build green):**

1. **Research spike (§9).** Throwaway `#[func]` + `tools_spike_test.gd`
   that calls a GDScript coroutine via `Callable::call`, inspects the
   return, and confirms the fallible signal-future awaits the `completed`
   signal from inside `godot::task::spawn`. *Output: Approach 1 (§6)
   confirmed or falsified, recorded back into this doc.*
2. **`NobodyWhoTool` + built-in `python`/`bash`** (no GDScript callable
   path yet — just the core-tool wrappers). Wire `create`'s `"tools"` key
   and `set_tools`/`reset_chat` to accept them. A model-backed test that
   gives the model `python_tool` and asks it to compute something.
3. **Sync GDScript tools.** `NobodyWhoTool.create` / `create_with_schema`,
   the per-tool main-thread loop, the worker-side `Fn` bridge, argument
   marshalling. Tests: a GDScript tool the model must call to answer, plus
   a `set_tools` mid-conversation test (replace the tool set between two
   `ask`s; verify the old tool's loop ends — old callable released — and
   the new tool is callable).
4. **Async GDScript tools.** Whatever mechanism §9 picked, layered onto the
   per-tool loop. Test: a tool that `await`s a `Timer` before returning.
5. **Failure hardening + re-entrancy guard.** The §5 hardening (is_valid
   check, timeout, fallible future — the fallible future lands with step 4)
   and the §3 `Arc<AtomicBool>` guard. Tests: a tool bound to a freed node;
   a tool that never completes (timeout fires, model sees the error, a
   later `ask` still works); a tool that calls back into its own chat
   (resolves `null` + error, no hang).
6. **User docs** (the `docs/godot/tool-calling.md` page) rewritten against
   the new `NobodyWhoTool.create(...)` API, with the re-entrancy rule, the
   hang-vs-freeze clarification, and the sandboxed-interpreter note (§7).

## 9. Research spike — async-tool detection

**Status: complete.** Approach 1 confirmed. The throwaway spike code
(`src/spike.rs`, `tests/tools_spike_test.gd`) is deleted now that the real
`tools.rs` implements the same mechanism.

**What was validated, and the result:**

1. **`GDScriptFunctionState` reachability — confirmed.** `Callable::callv`
   on an `await`-containing GDScript function returns `type=OBJECT`,
   `get_class() == "GDScriptFunctionState"`. The string class-name compare
   works (the class is a registered `GDCLASS(GDScriptFunctionState,
   RefCounted)` in ClassDB, so `get_class()` returns it even though gdext
   doesn't generate a Rust type for it).
2. **Fallible signal-future on `completed` — confirmed.**
   `Signal::from_object_signal(&state, "completed").to_fallible_future::<(Variant,)>().await`
   resolves to `Ok((value,))` where `value` is the coroutine's **return
   value** (a `STRING` holding `"async-result-99"` in the spike), on the
   gdext executor, from inside a `godot::task::spawn` future. `callv` does
   **not** block — it returns the state promptly and the result arrives
   later via `completed`.
3. **Sync return shape — confirmed.** `Callable::callv` on a sync function
   returns the plain value directly (`type=STRING`, value present, no
   `GDScriptFunctionState`). The sync/async distinction is a clean
   type-level branch: `OBJECT` + class name → async; anything else → sync.
4. **`callv` is the right entry.** gdext 0.5 exposes `Callable::callv`
   (vector arg list); there is no varargs `call`. It works for the
   dynamic-length positional call from inside the main-thread loop.
5. **`get_method_list` in gdext 0.5 — confirmed in source**
   (`godot-core-0.5.4/src/obj/script.rs:110`): returns `Vec<MethodInfo>`
   with `MethodInfo::arguments: Vec<PropertyInfo>` (`PropertyInfo::name`
   + `PropertyInfo::variant_type`). The schema generator ports cleanly.

**The gotcha that cost a round-trip (recorded so it doesn't bite again):**
the first spike run returned the declared type's default (empty `String` /
`null`) instead of a `GDScriptFunctionState`, which I misread as an engine
limitation. The real cause was the test harness: the suite Node was built
with `preload(...).new()` and **never added to the scene tree**, so
`get_tree()` inside the coroutine returned null, `create_timer(0.05)` was a
runtime error, the coroutine errored out instead of suspending, and
`callv` returned the declared type's default. **A coroutine that never
suspends (or errors before its first `await`) returns a plain value, not a
state** — which is correct engine behavior, not a limitation. The fix was
`runner.add_child(self)` so `get_tree()` is non-null. Lesson: when a spike
shows surprising behavior, verify the test setup produces the precondition
(here: a genuine suspension) before concluding anything about the engine.

**Primary sources that corroborate the confirmed design:**
- Godot `modules/gdscript/gdscript_vm.cpp` `OPCODE_AWAIT`: the VM sets
  `retvalue = gdfs` (the `GDScriptFunctionState`) unconditionally on
  suspension — no caller-dependent branch; native callers go through the
  same `Callable::callp → Object::callp → GDScriptInstance::callp →
  GDScriptFunction::call` path and receive the state as a `Variant`.
- `gdscript_function.h`: `GDCLASS(GDScriptFunctionState, RefCounted)` — a
  registered ClassDB class, so `get_class()` string compare works.
- godot-rust/gdext #1640: the working workaround there is exactly §6
  Approach 1, from Rust (cast to `Gd<Object>`, check for `completed`,
  connect, receive the return value).
- The old bindings' `get_class() == "GDScriptFunctionState"` error branch
  (`main:nobodywho/godot/src/lib.rs:~1434`) fired in production — the
  state does reach the caller.

## 10. What this phase does not include

- Auto-schema support for default args / typed arrays / per-arg
  descriptions (§4 — v1-deferred; the manual path covers all of them).
- Other class families: embeddings, cross-encoder, TTS, STT, prompt (Phase 4+).
- Core async constructors / terminal-outcome latch (Phase 5).
- The old `unsafe impl Send for SendCallable` — gone for good; the new design
  has no cross-thread GDScript calls (the callable runs on the main thread,
  the worker only blocks on a channel).
- A GDScript schema DSL (§4 — pass a Dictionary for now).

## 11. Decisions summary

| Question | Decision | Where |
|---|---|---|
| Re-entrancy policy | Detect-and-error guard **in Phase 3** (`Arc<AtomicBool>` per chat; nearly free with the per-tool-loop design) | §3 |
| JSON-schema source | Auto-generate from GDScript type hints + manual-schema escape hatch | §4 |
| Manual-path signature | `create_with_schema(name, description, schema, callable)` — explicit name, accepts lambdas/unbound callables | §4 |
| Argument order | Method info when bound (authoritative); schema `properties` insertion order for lambdas | §4 |
| Schema input shape for manual path | `Variant` (Dictionary or JSON string; `preserve_order` is load-bearing) | §4 |
| Async-tool detection | Approach 1 **confirmed by spike** (class-name + `FallibleSignalFuture::<(Variant,)>`); see §9 | §6, §9 |
| Result marshalling | `String` passes through; non-strings JSON-encoded via `variant_to_json` | §5 |
| Failure hardening | `is_valid()` check, `recv_timeout` (default 60s, per-tool override), fallible signal future, script errors → error string | §5 |
| Main-thread runner shape | One `godot::task::spawn` loop **per registered tool** (no `_process` Node, no global state) | §5 |
| Runner granularity (global / per-chat / per-tool / per-request) | Per-tool: lifetime derived from core's `Tool` closure via sender-drop; global & per-chat need registry sync; pure per-request impossible (`Callable: !Send`) | §5 |
| Where `Callable`s live | Main thread only, owned by their tool's loop future; never sent over a channel (`Callable`/`Variant` are `!Send`) | §5 |
| Channel types | worker→main: `tokio::sync::mpsc::unbounded`; main→worker result: per-request `std::sync::mpsc` | §5 |
| JSON→Variant arg conversion | Main-thread side (inside the tool loop), not worker side | §5 |
| Built-in python/bash tools | Included | §7 |
| Tool error convention | Return error *string* to model (not `null` to GDScript) | §7 |
