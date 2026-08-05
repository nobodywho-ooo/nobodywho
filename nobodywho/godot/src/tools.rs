use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use godot::builtin::VariantType;
use godot::prelude::*;

use nobodywho::tool_calling::Tool as CoreTool;

use crate::convert::{json_to_variant, variant_to_json};

/// Default wall-clock budget for a single tool call (sync or async). A tool
/// that hasn't produced a result by then resolves to an error string for the
/// model, and the worker unblocks. Override via the factories' `timeout_secs`.
const DEFAULT_TOOL_TIMEOUT_SECS: i64 = 60;

/// What a `NobodyWhoTool` knows how to build at registration time.
enum ToolSpec {
    /// A fully-built pure-Rust core tool (python/bash). Cloned per chat.
    Builtin(CoreTool),
    /// A GDScript tool. The core `Tool` (and its main-thread loop) is built
    /// fresh per registration, so each chat gets its own loop and its own
    /// re-entrancy flag capture.
    Script {
        name: String,
        description: String,
        schema: serde_json::Value,
        /// Argument names in positional-call order.
        order: Vec<String>,
        callable: Callable,
        timeout: Duration,
    },
}

/// A tool the model can call during generation. Build one with the static
/// factories and pass it to `NobodyWhoChat.create(..., {"tools": [...]})`,
/// `NobodyWhoChat.set_tools(...)`, or `NobodyWhoChat.reset_chat(...)`.
///
/// Three kinds:
/// - **GDScript tools** (`create` / `create_with_schema`) — a `Callable`. The
///   callable runs on the main thread; the worker blocks on a channel until
///   it returns. Sync and async (await-containing) GDScript methods both work.
/// - **`python()`** — a sandboxed in-process Python interpreter (no host
///   filesystem/network/env access; limited stdlib). Pure Rust.
/// - **`bash()`** — a sandboxed in-memory bash interpreter (no host
///   filesystem/network/env access). Pure Rust.
///
/// Re-entrancy: do **not** call back into the same `NobodyWhoChat` from
/// inside one of its own tools — the worker is blocked waiting for the tool
/// to return. Such calls fail fast with an error (and resolve to null)
/// instead of hanging. Calling a *different* chat from a tool is fine.
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWhoTool {
    /// `None` = construction failed (already reported via `godot_error!`);
    /// registration skips it.
    spec: Option<ToolSpec>,
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWhoTool {
    /// Create a tool from a `Callable` bound to a method on an Object. The
    /// JSON schema is auto-generated from the method's argument type hints
    /// (all arguments must have primitive type hints: bool, int, float,
    /// String, Array); the tool name is the method name. The method should
    /// return a `String` (non-strings are JSON-encoded). Async methods
    /// (containing `await`) are supported — the coroutine is awaited on the
    /// main thread. For lambdas, enums, nested objects, per-argument
    /// descriptions, or optional fields, use `create_with_schema`.
    /// `timeout_secs` bounds one call (default 60).
    #[func]
    fn create(
        callable: Callable,
        description: GString,
        #[opt(default = 60)] timeout_secs: i64,
    ) -> Gd<Self> {
        let spec = schema_from_callable(&callable).map(|(name, schema, order)| ToolSpec::Script {
            name,
            description: description.to_string(),
            schema,
            order,
            callable,
            timeout: timeout_duration(timeout_secs),
        });
        Self::from_spec(spec, "NobodyWhoTool.create")
    }

    /// Create a tool with an explicit JSON schema (a Dictionary or a JSON
    /// string). Use this when you need enums, nested objects, per-argument
    /// descriptions, or optional fields — or when the callable is a lambda
    /// / unbound method (the auto path can't reflect on those). `name` is
    /// the tool name. The schema's `properties` keys must match the
    /// callable's argument names. Argument order: the bound method's
    /// declaration order when the callable is a bound method; otherwise the
    /// schema's `properties` insertion order. `timeout_secs` bounds one
    /// call (default 60).
    #[func]
    fn create_with_schema(
        name: GString,
        description: GString,
        json_schema: Variant,
        callable: Callable,
        #[opt(default = 60)] timeout_secs: i64,
    ) -> Gd<Self> {
        let spec = validate_tool_name(&name.to_string()).and_then(|()| {
            let schema = parse_schema(&json_schema)?;
            let order = argument_order(&callable, &schema)?;
            Ok(ToolSpec::Script {
                name: name.to_string(),
                description: description.to_string(),
                schema,
                order,
                callable,
                timeout: timeout_duration(timeout_secs),
            })
        });
        Self::from_spec(spec, "NobodyWhoTool.create_with_schema")
    }

    /// A sandboxed in-process Python interpreter the model can call to run
    /// self-contained snippets. No host filesystem, network, or environment
    /// variable access; limited standard library. Works identically on all
    /// platforms. Optional limits: `max_duration_secs`, `max_memory_bytes`,
    /// `max_recursion_depth` (pass 0 / null for "no limit").
    #[func]
    fn python(
        #[opt(default = 0)] max_duration_secs: i64,
        #[opt(default = 0)] max_memory_bytes: i64,
        #[opt(default = 0)] max_recursion_depth: i64,
    ) -> Gd<Self> {
        let tool = CoreTool::python(
            opt_i64(max_duration_secs).map(|s| Duration::from_secs(s as u64)),
            opt_i64(max_memory_bytes).map(|i| i as usize),
            opt_i64(max_recursion_depth).map(|i| i as usize),
        );
        Self::from_spec(Ok(ToolSpec::Builtin(tool)), "NobodyWhoTool.python")
    }

    /// A sandboxed in-memory bash interpreter the model can call to run
    /// self-contained commands. In-memory filesystem only (no persistent
    /// state between calls); no network access; no host environment
    /// variables or host filesystem. Works identically on all platforms.
    /// Optional `max_commands` (pass 0 / null for "no limit").
    #[func]
    fn bash(#[opt(default = 0)] max_commands: i64) -> Gd<Self> {
        let tool = CoreTool::bash(opt_i64(max_commands).map(|i| i as usize));
        Self::from_spec(Ok(ToolSpec::Builtin(tool)), "NobodyWhoTool.bash")
    }
}

impl NobodyWhoTool {
    fn from_spec(spec: Result<ToolSpec, String>, ctx: &str) -> Gd<Self> {
        let spec = spec.map_err(|e| godot_error!("{ctx}: {e}")).ok();
        Gd::from_init_fn(|base| Self { spec, base })
    }

    /// Build the core `Tool` at registration time (main thread). GDScript
    /// tools spawn their per-registration main-thread loop here and capture
    /// the owning chat's re-entrancy flag; built-ins just clone. `None` for
    /// a tool whose construction failed.
    pub(crate) fn build_core_tool(&self, reentrancy_flag: Arc<AtomicBool>) -> Option<CoreTool> {
        match self.spec.as_ref()? {
            ToolSpec::Builtin(tool) => Some(tool.clone()),
            ToolSpec::Script {
                name,
                description,
                schema,
                order,
                callable,
                timeout,
            } => Some(build_gdscript_tool(
                name.clone(),
                description.clone(),
                schema.clone(),
                order.clone(),
                callable.clone(),
                *timeout,
                reentrancy_flag,
            )),
        }
    }
}

/// `i64` optional parameter -> `Option<i64>` (0 / null / unset => None).
fn opt_i64(v: i64) -> Option<i64> {
    (v != 0).then_some(v)
}

fn timeout_duration(secs: i64) -> Duration {
    Duration::from_secs(opt_i64(secs).unwrap_or(DEFAULT_TOOL_TIMEOUT_SECS).max(1) as u64)
}

// ====================================================================
// Schema generation from a Callable's method info
// ====================================================================

/// Look up the callable's method-argument dicts (`{name, type, ...}` each,
/// in declaration order), excluding arguments pre-filled with
/// `Callable.bind()` — Godot appends bound args after the provided ones, so
/// they fill the declaration from the right and the model must not supply
/// them. Errors for lambdas / unbound callables.
///
/// Note: the `args` field is a *typed* Array[Dictionary] on the engine side,
/// so the element type must be spelled out — `Array<Variant>` conversion
/// fails on typed arrays in gdext 0.5.
fn bound_method_args(callable: &Callable) -> Result<(String, Vec<VarDictionary>), String> {
    let method_name = callable
        .method_name()
        .ok_or("callable has no method name (anonymous function?)")?;
    let method_obj = callable
        .object()
        .ok_or("callable is not bound to an object (lambda or static method?)")?;
    let method_info = method_obj
        .get_method_list()
        .iter_shared()
        .find(|d| d.get_or_nil("name").to::<GString>() == GString::from(&method_name))
        .ok_or("method not found on the callable's object")?;
    let args: Array<VarDictionary> = method_info.get_or_nil("args").to();
    let unbound = args
        .len()
        .saturating_sub(callable.get_bound_arguments_count());
    Ok((
        method_name.to_string(),
        args.iter_shared().take(unbound).collect(),
    ))
}

/// Reflect on `callable`'s bound method and build a JSON schema + the
/// ordered argument-name list, from the method's type hints.
fn schema_from_callable(
    callable: &Callable,
) -> Result<(String, serde_json::Value, Vec<String>), String> {
    let (method_name, args) = bound_method_args(callable)
        .map_err(|e| format!("{e} — for lambdas/static methods, use create_with_schema"))?;

    let mut properties = serde_json::Map::new();
    let mut order: Vec<String> = Vec::new();

    for arg in args {
        let arg_name = arg.get_or_nil("name").to::<GString>().to_string();
        let json_type = match arg.get_or_nil("type").to::<VariantType>() {
            VariantType::BOOL => "boolean",
            VariantType::INT => "integer",
            VariantType::FLOAT => "number",
            VariantType::STRING => "string",
            VariantType::ARRAY => "array",
            VariantType::NIL => {
                return Err(format!(
                    "argument '{arg_name}' has no type hint — all arguments must have primitive type hints (or use create_with_schema)"
                ));
            }
            other => {
                return Err(format!(
                    "unsupported type for argument '{arg_name}': {other:?} — use create_with_schema for non-primitive types"
                ));
            }
        };
        properties.insert(arg_name.clone(), serde_json::json!({ "type": json_type }));
        order.push(arg_name);
    }

    let schema = serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": order,
    });
    Ok((method_name, schema, order))
}

/// Tool names end up inside the tool-call GBNF grammar and the chat
/// template; validate them here where the error is actionable, instead of
/// failing at ask-time inside core's grammar generation.
fn validate_tool_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let valid = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then_some(()).ok_or(format!(
        "invalid tool name '{name}' — must be a valid identifier ([a-zA-Z_][a-zA-Z0-9_]*)"
    ))
}

/// Parse a manual schema: a Dictionary, or a JSON string.
fn parse_schema(v: &Variant) -> Result<serde_json::Value, String> {
    match v.get_type() {
        VariantType::DICTIONARY => variant_to_json(v).map_err(|e| format!("bad schema: {e}")),
        VariantType::STRING => serde_json::from_str(&v.to::<GString>().to_string())
            .map_err(|e| format!("schema is not valid JSON: {e}")),
        other => Err(format!(
            "schema must be a Dictionary or a JSON string, got {other:?}"
        )),
    }
}

/// Positional-argument order for a manual-schema tool. Bound method → the
/// method's declared order (authoritative; schema `properties` must cover
/// the same names). Lambda/unbound → the schema's `properties` insertion
/// order (preserved — serde_json's `preserve_order` feature is enabled and
/// load-bearing here).
fn argument_order(callable: &Callable, schema: &serde_json::Value) -> Result<Vec<String>, String> {
    let props = schema
        .get("properties")
        .and_then(|p| p.as_object())
        .ok_or("schema must have an object-valued 'properties' key")?;

    match bound_method_args(callable) {
        Ok((_, args)) => {
            args.into_iter()
                .map(|arg| {
                    let name = arg.get_or_nil("name").to::<GString>().to_string();
                    props.contains_key(&name).then_some(name.clone()).ok_or(format!(
                    "schema 'properties' is missing argument '{name}' declared on the method"
                ))
                })
                .collect()
        }
        Err(_) => Ok(props.keys().cloned().collect()),
    }
}

// ====================================================================
// The per-tool main-thread loop + worker-side Fn bridge
// ====================================================================

/// A request from the worker to a tool's main-thread loop.
struct ToolRequest {
    /// The model's raw JSON arguments. `Send`; converted to Variants on the
    /// main-thread side (Variant is !Send).
    args_json: serde_json::Value,
    /// Worker blocks on the pair's `Receiver::recv_timeout`.
    result_tx: std::sync::mpsc::Sender<String>,
}

/// Build the core `Tool` for a GDScript tool: spawns the per-registration
/// main-thread loop (owning the `Callable`), and returns a core `Tool` whose
/// `function` closure captures only `Send` data (the loop's sender + the
/// chat's re-entrancy flag). The loop's lifetime is tied to core's ownership
/// of this closure via sender-drop: when `set_tools` replaces the tool vec
/// (or the chat is freed), core drops the closure -> sender drops -> `recv()`
/// yields `None` -> the loop ends and releases the `Callable`. Self-cleaning;
/// see `TOOLS_DESIGN.md` §5.
fn build_gdscript_tool(
    name: String,
    description: String,
    json_schema: serde_json::Value,
    order: Vec<String>,
    callable: Callable,
    timeout: Duration,
    reentrancy_flag: Arc<AtomicBool>,
) -> CoreTool {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ToolRequest>();

    // The main-thread dispatcher. Owns the Callable; never crosses threads.
    // Each request runs in its own sub-task (spawn is legal here: the
    // dispatcher itself runs on the main thread) so that a tool whose
    // coroutine never completes wedges only that one call — not the tool.
    // Without this, an abandoned await would park the loop forever and
    // every later call to this tool would queue behind it and time out.
    let loop_name: Arc<str> = name.clone().into();
    let loop_order: Arc<[String]> = order.into();
    godot::task::spawn(async move {
        while let Some(req) = rx.recv().await {
            let (callable, order, name) = (callable.clone(), loop_order.clone(), loop_name.clone());
            godot::task::spawn(async move {
                let result = run_tool_call(&callable, &order, &name, &req.args_json).await;
                // Err = worker gone or the call already timed out; the
                // orphaned result is dropped. Harmless.
                let _ = req.result_tx.send(result);
            });
        }
    });

    // The worker-side closure. Captures only Send data. Blocks the worker
    // (a std thread) on recv_timeout — never the main thread. The
    // re-entrancy flag is set for the duration so chat methods called from
    // inside the tool fail fast instead of hanging (TOOLS_DESIGN.md §3);
    // it is set *before* the send so the main thread can't observe it unset.
    let closure_name = name.clone();
    let func: Arc<dyn Fn(serde_json::Value) -> String + Send + Sync> = Arc::new(move |args_json| {
        let (result_tx, result_rx) = std::sync::mpsc::channel::<String>();
        reentrancy_flag.store(true, Ordering::Release);
        let result = if tx
            .send(ToolRequest {
                args_json,
                result_tx,
            })
            .is_err()
        {
            "Error: tool runner gone".into()
        } else {
            result_rx
                .recv_timeout(timeout)
                .unwrap_or_else(|_| format!("Error: tool '{closure_name}' timed out"))
        };
        reentrancy_flag.store(false, Ordering::Release);
        result
    });

    CoreTool::new(name, description, json_schema, func)
}

/// Run one tool call on the main thread: convert JSON args to positional
/// Variants, call the callable, await the coroutine if async, stringify the
/// result. Returns the result string for the model (or an error string).
async fn run_tool_call(
    callable: &Callable,
    order: &[String],
    name: &str,
    args_json: &serde_json::Value,
) -> String {
    if !callable.is_valid() {
        return format!("Error: tool '{name}' is no longer valid (its object was freed)");
    }
    let Some(obj) = args_json.as_object() else {
        return "Error: bad arguments — expected a JSON object".into();
    };

    // Positional args in declaration order; missing args -> nil.
    let args: VarArray = order
        .iter()
        .map(|prop| obj.get(prop).map_or(Variant::nil(), json_to_variant))
        .collect();

    let res: Variant = callable.callv(&args);

    // Async tool: callv returned a GDScriptFunctionState. Await its
    // `completed` signal (1 arg = the coroutine's return value) via the
    // *fallible* future — the panicking `to_future` is banned because a
    // panic in a godot::task future hangs silently (TOOLS_DESIGN.md §5/§6).
    match as_gdscript_function_state(&res) {
        Some(state) => {
            let signal = Signal::from_object_signal(&state, "completed");
            match signal.to_fallible_future::<(Variant,)>().await {
                Ok((value,)) => stringify_result(&value, name),
                Err(_) => format!("Error: tool '{name}' coroutine was freed mid-await"),
            }
        }
        None => stringify_result(&res, name),
    }
}

/// If `v` is an Object whose class is `GDScriptFunctionState`, return it.
/// The class is a registered ClassDB type, so `get_class()` string compare
/// works even though gdext doesn't generate a Rust type for it.
fn as_gdscript_function_state(v: &Variant) -> Option<Gd<RefCounted>> {
    let obj = v.try_to::<Gd<RefCounted>>().ok()?;
    (obj.get_class() == "GDScriptFunctionState").then_some(obj)
}

/// Marshal a tool result to the string the model sees. Strings pass through
/// as-is (the documented `-> String` contract); non-strings are JSON-encoded
/// (models are trained on JSON tool results, not Godot's `str()` repr). Nil —
/// a tool without a return value, or a script error during the call — is
/// surfaced as an error string rather than a fake success.
fn stringify_result(v: &Variant, name: &str) -> String {
    match v.get_type() {
        VariantType::STRING => v.to::<GString>().to_string(),
        VariantType::NIL => {
            format!(
                "Error: tool '{name}' returned null (no return value, or script error during call)"
            )
        }
        _ => match variant_to_json(v) {
            Ok(j) => j.to_string(),
            Err(e) => format!("Error: tool '{name}' result not JSON-encodable: {e}"),
        },
    }
}
