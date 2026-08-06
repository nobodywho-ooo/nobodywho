use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use godot::builtin::VariantType;
use godot::prelude::*;

use nobodywho::tool_calling::Tool as CoreTool;

use crate::convert::{json_to_variant, variant_to_json};

/// Time budget for one tool call. Override via the factories' `timeout_secs`.
const DEFAULT_TOOL_TIMEOUT_SECS: i64 = 60;

/// What a `NobodyWhoTool` builds when it is registered to a chat.
enum ToolSpec {
    /// Ready-made core tool (python/bash). Cloned per chat.
    Builtin(CoreTool),
    /// A GDScript tool; built fresh per registration.
    Script {
        name: String,
        description: String,
        schema: serde_json::Value,
        /// Argument names in call order.
        order: Vec<String>,
        callable: Callable,
        timeout: Duration,
    },
}

/// A tool the model can call during generation. Build one with the static
/// factories, then pass it to `NobodyWhoChat.create`, `set_tools`, or
/// `reset_chat`. GDScript tools run on the main thread; async ones work too.
///
/// Do not call back into the same chat from inside one of its own tools —
/// such calls fail with an error. Calling a different chat is fine.
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWhoTool {
    /// `None` = construction failed (already reported); registration skips it.
    spec: Option<ToolSpec>,
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWhoTool {
    /// Create a tool from a method `Callable`. The schema comes from the
    /// method's type hints; the tool name is the method name. All arguments
    /// need primitive type hints (bool, int, float, String, Array).
    /// For lambdas or richer schemas, use `create_with_schema`.
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

    /// Create a tool with an explicit JSON schema (Dictionary or JSON string).
    /// For lambdas, enums, nested objects, arg descriptions, or optional
    /// fields. The schema's `properties` keys must match the callable's
    /// argument names.
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

    /// A sandboxed Python interpreter tool. No host filesystem, network, or
    /// env access. Pass 0 for "no limit" on any of the limits.
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

    /// A sandboxed in-memory bash interpreter tool. No host filesystem,
    /// network, or env access. Pass 0 for "no limit".
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

    /// Build the core `Tool` at registration time (main thread). Script
    /// tools spawn their dispatcher here; built-ins just clone.
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

/// Optional int parameter: 0 means "not set".
fn opt_i64(v: i64) -> Option<i64> {
    (v != 0).then_some(v)
}

fn timeout_duration(secs: i64) -> Duration {
    Duration::from_secs(opt_i64(secs).unwrap_or(DEFAULT_TOOL_TIMEOUT_SECS).max(1) as u64)
}

// ====================================================================
// Schema generation from a Callable's method info
// ====================================================================

/// The callable's method-argument dicts (`{name, type, ...}`), in
/// declaration order. Skips args pre-filled with `Callable.bind()` (they
/// fill from the right). Errors for lambdas / unbound callables.
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
    // "args" is a typed Array[Dictionary]; `Array<Variant>` would panic here.
    let args: Array<VarDictionary> = method_info.get_or_nil("args").to();
    let unbound = args
        .len()
        .saturating_sub(callable.get_bound_arguments_count());
    Ok((
        method_name.to_string(),
        args.iter_shared().take(unbound).collect(),
    ))
}

/// Build a JSON schema + argument order from the method's type hints.
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

/// Tool names end up in the tool-call grammar; catch bad ones here instead
/// of failing confusingly at ask-time.
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

/// Argument order for a manual-schema tool: the method's declared order for
/// bound methods, otherwise the schema's `properties` order (serde_json's
/// `preserve_order` feature keeps it — don't remove that feature).
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

/// One tool call, sent from the worker to the tool's main-thread dispatcher.
struct ToolRequest {
    args_json: serde_json::Value,
    result_tx: std::sync::mpsc::Sender<String>,
}

/// Build a core `Tool` for a GDScript tool: spawn a main-thread dispatcher
/// that owns the `Callable`, and return a core `Tool` whose closure just
/// sends requests to it. When core drops the closure, the sender drops and
/// the dispatcher ends on its own. Details in TOOLS_DESIGN.md §5.
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

    // Main-thread dispatcher; owns the Callable. Each request runs in its
    // own sub-task, so a coroutine that never finishes only wedges that one
    // call — later calls to the tool still work.
    let loop_name: Arc<str> = name.clone().into();
    let loop_order: Arc<[String]> = order.into();
    godot::task::spawn(async move {
        while let Some(req) = rx.recv().await {
            let (callable, order, name) = (callable.clone(), loop_order.clone(), loop_name.clone());
            godot::task::spawn(async move {
                let result = run_tool_call(&callable, &order, &name, &req.args_json).await;
                // Send fails if the worker already timed out; that's fine.
                let _ = req.result_tx.send(result);
            });
        }
    });

    // Worker-side closure: blocks the worker thread (never the main thread)
    // until the dispatcher sends the result back. The re-entrancy flag is
    // set for the duration, before the send, so a tool that calls back into
    // its own chat fails fast instead of hanging.
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

/// Run one tool call on the main thread: JSON args -> Variants, call the
/// callable, await the coroutine if async, stringify. Errors become error
/// strings for the model.
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

    // Positional args in declaration order; missing args become nil.
    let args: VarArray = order
        .iter()
        .map(|prop| obj.get(prop).map_or(Variant::nil(), json_to_variant))
        .collect();

    let res: Variant = callable.callv(&args);

    // Async tool: await the coroutine's `completed` signal (its one arg is
    // the return value). Must be the fallible future — the plain one panics
    // if the state is freed, and panics in godot tasks hang silently.
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

/// Returns the object if `v` is a GDScriptFunctionState (a suspended
/// coroutine). gdext has no generated type for it, so compare by class name.
fn as_gdscript_function_state(v: &Variant) -> Option<Gd<RefCounted>> {
    let obj = v.try_to::<Gd<RefCounted>>().ok()?;
    (obj.get_class() == "GDScriptFunctionState").then_some(obj)
}

/// The string the model sees: Strings pass through, other values are
/// JSON-encoded, nil becomes an error (no return value, or script error).
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
