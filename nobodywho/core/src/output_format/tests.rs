use super::grammar::{escape_lark_string, json_schema_for_llguidance, properties, sanitize_lark};
use super::*;
use crate::test_utils::{load_gemma4_vocab, load_test_vocab};
use crate::tool_calling::{Tool, ToolCall};
use serde_json::{json, Value};
use std::sync::Arc;

// ============================================================================
// Fixtures
// ============================================================================

/// A vocabulary with exactly the given special tokens, which ends generation
/// with `EOG`.
struct FakeVocab(Vec<(&'static str, SpecialToken)>);

impl Vocab for FakeVocab {
    fn special_token(&self, text: &str) -> Option<SpecialToken> {
        self.0
            .iter()
            .find(|(t, _)| *t == text)
            .map(|&(_, token)| token)
    }

    fn end_of_generation(&self) -> Vec<LlamaToken> {
        vec![EOG]
    }
}

const BEGIN: LlamaToken = LlamaToken(1000);
const END: LlamaToken = LlamaToken(1001);
const THINK: LlamaToken = LlamaToken(1002);
const UNTHINK: LlamaToken = LlamaToken(1003);
const EOG: LlamaToken = LlamaToken(1004);
/// How the splitter tests spell the end of generation.
const EOG_TEXT: &str = "<eog>";

/// The special tokens `format` needs, as text.
fn specials(format: &OutputFormat) -> Vec<(&'static str, SpecialToken)> {
    let text = |id| SpecialToken { id, control: false };
    let syntax = format.tool_calls;
    let mut specials = vec![(syntax.begin, text(BEGIN))];
    specials.extend(syntax.end.map(|end| (end, text(END))));
    if let Some(thinking) = format.thinking {
        specials.extend([(thinking.begin, text(THINK)), (thinking.end, text(UNTHINK))]);
    }
    specials
}

/// `format` over a vocabulary that has its markers as text.
fn resolve(format: &OutputFormat) -> ResolvedFormat {
    ResolvedFormat::new(*format, &FakeVocab(specials(format))).unwrap()
}

/// Every format.
fn formats() -> impl Iterator<Item = OutputFormat> {
    [function_gemma, gemma4, qwen35, qwen3, ministral3, lfm2]
        .into_iter()
        .map(|f| f())
}

fn tool(name: &str, schema: Value) -> Tool {
    Tool::new(name, "", schema, Arc::new(|_| String::new()))
}

fn tools() -> Vec<Tool> {
    vec![
        tool(
            "get_weather",
            json!({
                "type": "object",
                "properties": {
                    "location": { "type": "string" },
                    "unit": { "type": "string", "enum": ["celsius", "fahrenheit"] },
                },
                "required": ["location"],
            }),
        ),
        tool(
            "calculate",
            json!({
                "type": "object",
                "properties": {
                    "exact": { "type": "boolean" },
                    "x": { "type": "number" },
                    "y": { "type": "integer" },
                },
                "required": ["x", "y"],
            }),
        ),
        tool(
            "create_event",
            json!({
                "type": "object",
                "properties": {
                    "place": {
                        "type": "object",
                        "properties": {
                            "city": { "type": "string" },
                            "floor": { "type": "integer" },
                        },
                        "required": ["city", "floor"],
                    },
                    "tags": { "type": "array", "items": { "type": "string" } },
                },
                "required": ["place", "tags"],
            }),
        ),
        tool("get_time", json!({ "type": "object", "properties": {} })),
        // Names Lark can't take as they are, for values that end where a
        // format's own markers could begin.
        tool(
            "set_task",
            json!({
                "type": "object",
                "properties": {
                    "activeForm": { "type": "string" },
                    "mode": { "type": "string", "enum": ["plain", "with\nnewline"] },
                },
                "required": ["activeForm", "mode"],
            }),
        ),
    ]
}

fn call(name: &str, arguments: Value) -> ToolCall {
    ToolCall {
        name: name.to_string(),
        arguments,
    }
}

/// Calls every format can write. `create_event` has nested values, which only
/// JSON and JSON-like values can hold.
fn calls(format: &OutputFormat) -> Vec<ToolCall> {
    let mut calls = vec![
        call("get_weather", json!({ "location": "Paris" })),
        call(
            "get_weather",
            json!({ "location": "count < 5, \"quoted\"\nnext line", "unit": "celsius" }),
        ),
        call("calculate", json!({ "exact": false, "x": 1.5, "y": -2 })),
        call("get_time", json!({})),
    ];
    let nests = match format.tool_calls.call {
        CallSyntax::JsonObject { .. } => true,
        CallSyntax::Parts(parts) => match parts.args {
            ArgsSyntax::Json => true,
            ArgsSyntax::KeyValue { value, .. } => {
                matches!(value, ValueSyntax::Json | ValueSyntax::JsonLike { .. })
            }
        },
    };
    if nests {
        calls.push(call(
            "create_event",
            json!({ "place": { "city": "NYC", "floor": 3 }, "tags": ["work", "urgent"] }),
        ));
    }
    calls
}

// ============================================================================
// Rendering, as each format's chat template would
// ============================================================================

fn render_value(syntax: ValueSyntax, value: &Value) -> String {
    let text = |value: &Value| match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    match syntax {
        ValueSyntax::Json => value.to_string(),
        ValueSyntax::Raw => text(value),
        ValueSyntax::Delimited(quote) => format!("{quote}{}{quote}", text(value)),
        ValueSyntax::JsonLike { quote } => render_json_like(value, quote),
    }
}

fn render_json_like(value: &Value, quote: &str) -> String {
    match value {
        Value::String(s) => format!("{quote}{s}{quote}"),
        Value::Array(items) => {
            let items: Vec<_> = items.iter().map(|v| render_json_like(v, quote)).collect();
            format!("[{}]", items.join(","))
        }
        Value::Object(map) => {
            let entries: Vec<_> = map
                .iter()
                .map(|(k, v)| format!("{k}:{}", render_json_like(v, quote)))
                .collect();
            format!("{{{}}}", entries.join(","))
        }
        other => other.to_string(),
    }
}

fn render_call(format: &OutputFormat, call: &ToolCall) -> String {
    let syntax = match format.tool_calls.call {
        CallSyntax::Parts(parts) => parts,
        CallSyntax::JsonObject {
            name_key,
            arguments_key,
        } => {
            let name = Value::String(call.name.clone());
            return format!(
                "\n{{\"{name_key}\": {name}, \"{arguments_key}\": {}}}\n",
                call.arguments
            );
        }
    };
    let args = match syntax.args {
        ArgsSyntax::Json => call.arguments.to_string(),
        ArgsSyntax::KeyValue {
            before_key,
            after_key,
            after_value,
            separator,
            value,
        } => {
            // In schema order, which is the order the grammar allows.
            let schema = &tools()
                .into_iter()
                .find(|t| t.name == call.name)
                .unwrap()
                .json_schema;
            let args: Vec<_> = properties(schema)
                .filter_map(|(key, _)| {
                    let v = call.arguments.get(key)?;
                    Some(format!(
                        "{before_key}{key}{after_key}{}{after_value}",
                        render_value(value, v)
                    ))
                })
                .collect();
            args.join(separator)
        }
    };
    format!(
        "{}{}{}{args}{}",
        syntax.before_name, call.name, syntax.after_name, syntax.after_args
    )
}

/// The text of a block of tool calls holding `calls`, without its begin and
/// end markers.
fn render_tool_calls(format: &OutputFormat, calls: &[ToolCall]) -> String {
    let calls: Vec<_> = calls.iter().map(|c| render_call(format, c)).collect();
    match format.tool_calls.list {
        Some(list) => format!("{}{}{}", list.open, calls.join(list.separator), list.close),
        None => {
            assert_eq!(calls.len(), 1, "{} has one call per block", format.name);
            calls[0].clone()
        }
    }
}

/// A full response making `calls`, markers included.
fn render_response(format: &OutputFormat, calls: &[ToolCall]) -> String {
    let syntax = format.tool_calls;
    let end = syntax.end.unwrap_or("");
    let tool_calls =
        |calls: &[ToolCall]| format!("{}{}{end}", syntax.begin, render_tool_calls(format, calls));
    match syntax.list {
        Some(_) => tool_calls(calls),
        None => calls
            .iter()
            .map(|c| tool_calls(std::slice::from_ref(c)))
            .collect(),
    }
}

// ============================================================================
// Reading
// ============================================================================

#[test]
fn every_format_reads_back_what_it_writes() {
    for format in formats() {
        let resolved = resolve(&format);
        for call in calls(&format) {
            let text = render_tool_calls(&format, std::slice::from_ref(&call));
            assert_eq!(
                resolved.parse_tool_calls(&text, &tools()),
                Ok(vec![call]),
                "{} failed on {text:?}",
                format.name
            );
        }
    }
}

#[test]
fn reads_what_the_old_handlers_were_tested_on() {
    let cases: &[(&OutputFormat, &str, Vec<ToolCall>)] = &[
        (
            &qwen3(),
            r#"{"name": "tool1", "arguments": {"a": 1}}"#,
            vec![call("tool1", json!({ "a": 1 }))],
        ),
        (
            &qwen35(),
            "\n<function=get_weather>\n<parameter=location>\nsunny\n\n</parameter>\n</function>\n",
            vec![call("get_weather", json!({ "location": "sunny\n" }))],
        ),
        (
            &function_gemma(),
            "call:calculate{x:<escape>10<escape>, y:<escape>20<escape>, op:<escape>add<escape>}",
            vec![call("calculate", json!({ "x": 10, "y": 20, "op": "add" }))],
        ),
        (
            &function_gemma(),
            "call:write_file{content:<escape>line1\nline2<escape>}",
            vec![call("write_file", json!({ "content": "line1\nline2" }))],
        ),
        (
            &gemma4(),
            "call:search{query:<|\"|>rust lang<|\"|>,limit:10,exact:false}",
            vec![call(
                "search",
                json!({ "query": "rust lang", "limit": 10, "exact": false }),
            )],
        ),
        (
            &ministral3(),
            r#"sparklify[ARGS]{"text": "JULEMAND"}"#,
            vec![call("sparklify", json!({ "text": "JULEMAND" }))],
        ),
        (
            &lfm2(),
            r#"[get_weather(location="Paris"), set_flag(on=True, off=False, x=None, name='Bob')]"#,
            vec![
                call("get_weather", json!({ "location": "Paris" })),
                call(
                    "set_flag",
                    json!({ "on": true, "off": false, "x": null, "name": "Bob" }),
                ),
            ],
        ),
    ];
    for (format, text, expected) in cases {
        assert_eq!(
            resolve(format).parse_tool_calls(text, &tools()),
            Ok(expected.clone()),
            "{} failed on {text:?}",
            format.name
        );
    }
}

#[test]
fn a_json_call_can_name_its_arguments_first() {
    let text = r#"{"arguments": {"location": "Oslo"}, "name": "get_weather"}"#;
    assert_eq!(
        resolve(&qwen3()).parse_tool_calls(text, &tools()),
        Ok(vec![call("get_weather", json!({ "location": "Oslo" }))])
    );
}

#[test]
fn values_are_read_by_their_schema_type() {
    // A string that looks like a number stays a string when the schema says so.
    let text = "\n<function=get_weather>\n<parameter=location>\n123\n</parameter>\n</function>\n";
    assert_eq!(
        resolve(&qwen35()).parse_tool_calls(text, &tools()),
        Ok(vec![call("get_weather", json!({ "location": "123" }))])
    );
}

/// A value is read as JSON only if the schema allows what that gives, and
/// otherwise kept as the text it is.
#[test]
fn values_keep_the_types_their_schemas_allow() {
    let tools = [tool(
        "pick",
        json!({
            "type": "object",
            "properties": {
                "n": { "type": ["integer", "null"] },
                "m": { "anyOf": [{ "type": "integer" }, { "type": "null" }] },
                "s": { "type": "string" },
                "u": { "description": "anything" },
                "e": { "enum": ["1", "2"] },
                "o": { "type": ["string", "null"] },
                "b": { "type": "boolean" },
            },
        }),
    )];
    let text = "call:pick{n:<escape>5<escape>,m:<escape>6<escape>,\
                s:<escape>7<escape>,u:<escape>8<escape>,e:<escape>1<escape>,\
                o:<escape>12345<escape>,b:<escape>true<escape>}";
    assert_eq!(
        resolve(&function_gemma()).parse_tool_calls(text, &tools),
        Ok(vec![call(
            "pick",
            json!({ "n": 5, "m": 6, "s": "7", "u": "8", "e": "1", "o": "12345", "b": true })
        )])
    );
}

#[test]
fn reads_whitespace_the_template_would_not_have_written() {
    let text = r#"{"name":"get_time","arguments":{}}"#;
    assert_eq!(
        resolve(&qwen3()).parse_tool_calls(text, &tools()),
        Ok(vec![call("get_time", json!({}))])
    );
    let text = "call:calculate{x:<escape>1<escape>,y:<escape>2<escape>}";
    assert_eq!(
        resolve(&function_gemma()).parse_tool_calls(text, &tools()),
        Ok(vec![call("calculate", json!({ "x": 1, "y": 2 }))])
    );
}

/// A raw value's whitespace is the model's, like the indentation of code.
#[test]
fn raw_values_keep_their_whitespace() {
    let call = call("get_weather", json!({ "location": "\n    x = 1\n\n" }));
    let text = render_tool_calls(&qwen35(), std::slice::from_ref(&call));
    assert_eq!(
        resolve(&qwen35()).parse_tool_calls(&text, &tools()),
        Ok(vec![call])
    );
}

#[test]
fn says_where_a_block_stops_making_sense() {
    let error = resolve(&gemma4())
        .parse_tool_calls("call:get_time{", &tools())
        .unwrap_err();
    assert_eq!(error.at, "call:get_time{".len());

    let error = resolve(&qwen3())
        .parse_tool_calls(
            "\n{\"name\": \"get_time\", \"arguments\": {}}\ntrailing",
            &tools(),
        )
        .unwrap_err();
    assert_eq!(error.expected, "the end of the block");
}

// ============================================================================
// Splitting
// ============================================================================

/// Feeds `pieces` through a splitter for a response to `prompt`, and returns
/// the pieces and warnings. Markers get their tokens, and anything else is
/// token 1. Checks that every token ends up in exactly one piece, in order.
fn split_with_warnings(
    format: &OutputFormat,
    prompt: &str,
    pieces: &[&str],
) -> (Vec<Piece>, Vec<Warning>) {
    let resolved = resolve(format);
    let tools = tools();
    let mut splitter = resolved.splitter(tools, prompt);
    let token = |piece: &str| {
        specials(format)
            .into_iter()
            .find(|(marker, _)| *marker == piece)
            .map(|(_, token)| token.id)
            .unwrap_or(if piece == EOG_TEXT {
                EOG
            } else {
                LlamaToken(1)
            })
    };
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    for &piece in pieces {
        let (new, warning) = splitter.push(token(piece), piece.as_bytes());
        out.extend(new);
        warnings.extend(warning);
    }
    let (last, warning) = splitter.finish();
    out.extend(last);
    warnings.extend(warning);

    let ids: Vec<LlamaToken> = out.iter().flat_map(|p| &p.tokens).map(|t| t.id).collect();
    let pushed: Vec<LlamaToken> = pieces.iter().map(|&piece| token(piece)).collect();
    assert_eq!(
        ids, pushed,
        "every token is in one piece, in order: {out:?}"
    );
    let text: String = out
        .iter()
        .flat_map(|p| &p.tokens)
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(text, pieces.concat());
    (out, warnings)
}

/// `split_with_warnings`'s pieces.
fn split_after(format: &OutputFormat, prompt: &str, pieces: &[&str]) -> Vec<Piece> {
    split_with_warnings(format, prompt, pieces).0
}

fn split(format: &OutputFormat, pieces: &[&str]) -> Vec<Piece> {
    split_after(format, "", pieces)
}

fn kinds(pieces: &[Piece]) -> Vec<PieceKind> {
    pieces.iter().map(|piece| piece.kind.clone()).collect()
}

/// Each piece, with the text of its tokens.
fn with_tokens(pieces: &[Piece]) -> Vec<(PieceKind, String)> {
    pieces
        .iter()
        .map(|piece| {
            let text = piece.tokens.iter().map(|t| t.text.as_str()).collect();
            (piece.kind.clone(), text)
        })
        .collect()
}

fn open(item: Item) -> PieceKind {
    PieceKind::Open(item)
}

fn open_call(name: &str) -> PieceKind {
    PieceKind::Open(Item::ToolCall { name: name.into() })
}

fn delta(text: &str) -> PieceKind {
    PieceKind::Delta(text.into())
}

const CLOSE: PieceKind = PieceKind::Close;

fn end(cut_off: bool) -> PieceKind {
    PieceKind::End { cut_off }
}

#[test]
fn splits_text_from_calls() {
    let pieces = [
        "Let me ",
        "check.",
        "<tool_call>",
        "\n",
        "{\"name\": \"get_time\", ",
        "\"arguments\": {}}",
        "\n",
        "</tool_call>",
        "\n",
    ];
    assert_eq!(
        kinds(&split(&qwen3(), &pieces)),
        vec![
            open(Item::Text),
            delta("Let me "),
            delta("check."),
            CLOSE,
            open_call("get_time"),
            delta("{}"),
            CLOSE,
            open(Item::Text),
            delta("\n"),
            CLOSE,
            end(true),
        ]
    );
}

/// Markers are with the item they begin or end, along with the formatting
/// around them. A token goes with the first piece its text is part of.
#[test]
fn tokens_go_with_the_pieces_they_write() {
    let pieces = ["<think>", "\nHmm", ".\n", "</think>", "\n\nHi", EOG_TEXT];
    assert_eq!(
        with_tokens(&split(&qwen3(), &pieces)),
        vec![
            (open(Item::Thinking), "<think>\nHmm".into()),
            (delta("Hmm"), "".into()),
            (delta("."), ".\n".into()),
            (CLOSE, "</think>\n\nHi".into()),
            (open(Item::Text), "".into()),
            (delta("Hi"), "".into()),
            (CLOSE, "".into()),
            (end(false), EOG_TEXT.into()),
        ]
    );
}

/// Each call in a list gets the tokens of its own text, and the list's syntax
/// goes with the call after it, or the last call's end.
#[test]
fn calls_in_one_block_get_their_own_tokens() {
    let pieces = [
        "<|tool_call_start|>",
        "[",
        "get_time()",
        ", ",
        "get_weather(location=\"Oslo\")",
        "]",
        "<|tool_call_end|>",
    ];
    assert_eq!(
        with_tokens(&split(&lfm2(), &pieces)),
        vec![
            (open_call("get_time"), "<|tool_call_start|>[".into()),
            (delta("{}"), "get_time()".into()),
            (CLOSE, "".into()),
            (open_call("get_weather"), ", ".into()),
            (
                delta(r#"{"location":"Oslo"}"#),
                "get_weather(location=\"Oslo\")".into()
            ),
            (CLOSE, "]<|tool_call_end|>".into()),
            (end(true), "".into()),
        ]
    );
}

#[test]
fn markers_spelled_in_text_are_text() {
    // The characters of `<tool_call>` arriving as ordinary tokens.
    let pieces = ["use ", "<tool", "_call>", " to call tools"];
    assert_eq!(
        kinds(&split(&qwen3(), &pieces)),
        vec![
            open(Item::Text),
            delta("use "),
            delta("<tool"),
            delta("_call>"),
            delta(" to call tools"),
            CLOSE,
            end(true),
        ]
    );
}

#[test]
fn a_block_without_an_end_runs_to_the_next() {
    let pieces = [
        "[TOOL_CALLS]",
        "get_time[ARGS]{}",
        "[TOOL_CALLS]",
        "get_weather[ARGS]",
        "{\"location\": \"Oslo\"}",
        EOG_TEXT,
    ];
    assert_eq!(
        kinds(&split(&ministral3(), &pieces)),
        vec![
            open_call("get_time"),
            delta("{}"),
            CLOSE,
            open_call("get_weather"),
            delta(r#"{"location":"Oslo"}"#),
            CLOSE,
            end(false),
        ]
    );
}

/// The formatting before a block that isn't calls is text like the block.
#[test]
fn a_broken_block_keeps_the_formatting_before_it() {
    let pieces = [
        "Let me check.",
        "\n",
        "<tool_call>",
        "\nnot json\n",
        "</tool_call>",
        EOG_TEXT,
    ];
    let text: String = split(&qwen3(), &pieces)
        .into_iter()
        .filter_map(|piece| match piece.kind {
            PieceKind::Delta(text) => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(text, "Let me check.\n<tool_call>\nnot json\n</tool_call>");
}

#[test]
fn a_broken_block_comes_back_as_its_text() {
    let pieces = ["<|tool_call>", "call:get_time(", "<tool_call|>"];
    let (pieces, warnings) = split_with_warnings(&gemma4(), "", &pieces);
    assert!(
        matches!(warnings[..], [Warning::Malformed(_)]),
        "{warnings:?}"
    );
    assert_eq!(
        kinds(&pieces),
        [
            open(Item::Text),
            delta("<|tool_call>call:get_time(<tool_call|>"),
            CLOSE,
            end(true),
        ]
    );
}

/// A block the model didn't finish holds no calls, even one it could be read
/// as, so a call cut off by a stop or the token limit doesn't run.
#[test]
fn a_cut_off_block_is_text() {
    let pieces = ["<|tool_call_start|>", "[get_time()]"];
    assert_eq!(
        kinds(&split(&lfm2(), &pieces)),
        vec![
            open(Item::Text),
            delta("<|tool_call_start|>[get_time()]"),
            CLOSE,
            end(true)
        ]
    );
    let pieces = ["<|tool_call_start|>", "[get_time()]", EOG_TEXT];
    assert_eq!(
        kinds(&split(&lfm2(), &pieces)),
        vec![open_call("get_time"), delta("{}"), CLOSE, end(false)]
    );
}

/// A response written exactly as Qwen3's template would write it.
#[test]
fn template_formatting_is_neither_text_nor_reasoning() {
    let block = [
        "<tool_call>",
        "\n",
        "{\"name\": \"get_time\", \"arguments\": {}}",
        "\n",
        "</tool_call>",
    ];
    let mut pieces = vec![
        "<think>",
        "\n",
        "Hmm",
        "\n",
        "</think>",
        "\n",
        "\n",
        "Let me check.",
        "\n",
    ];
    pieces.extend(block);
    pieces.push("\n");
    pieces.extend(block);
    pieces.push(EOG_TEXT);
    let call = r#"{"name": "get_time", "arguments": {}}"#;
    assert_eq!(
        with_tokens(&split(&qwen3(), &pieces)),
        vec![
            (open(Item::Thinking), "<think>\n".into()),
            (delta("Hmm"), "Hmm".into()),
            (CLOSE, "\n</think>\n\n".into()),
            (open(Item::Text), "".into()),
            (delta("Let me check."), "Let me check.".into()),
            (CLOSE, "".into()),
            (open_call("get_time"), "\n<tool_call>\n".into()),
            (delta("{}"), call.into()),
            (CLOSE, "\n</tool_call>".into()),
            (open_call("get_time"), "\n<tool_call>\n".into()),
            (delta("{}"), call.into()),
            (CLOSE, "\n</tool_call>".into()),
            (end(false), EOG_TEXT.into()),
        ]
    );
}

/// Qwen3.5 writes `\n\n` between text and a call, but `\n` between calls.
#[test]
fn formatting_around_blocks_is_per_format() {
    let block = [
        "<tool_call>",
        "\n<function=get_time>\n</function>\n",
        "</tool_call>",
    ];
    let mut pieces = vec!["Checking.", "\n\n"];
    pieces.extend(block);
    pieces.push("\n");
    pieces.extend(block);
    pieces.push(EOG_TEXT);
    assert_eq!(
        kinds(&split(&qwen35(), &pieces)),
        vec![
            open(Item::Text),
            delta("Checking."),
            CLOSE,
            open_call("get_time"),
            delta("{}"),
            CLOSE,
            open_call("get_time"),
            delta("{}"),
            CLOSE,
            end(false),
        ]
    );
}

#[test]
fn only_the_formatting_is_removed() {
    // Whitespace the template doesn't write is the model's own. Whitespace
    // that could be formatting is held back until it's clear it isn't.
    let pieces = [
        "<think>",
        "\n\n  indented",
        "\n",
        "</think>",
        "\n\n",
        "    code\n",
        "\n",
        "more",
        EOG_TEXT,
    ];
    assert_eq!(
        kinds(&split(&qwen3(), &pieces)),
        vec![
            open(Item::Thinking),
            delta("\n  indented"),
            CLOSE,
            open(Item::Text),
            delta("    code"),
            delta("\n"),
            delta("\nmore"),
            CLOSE,
            end(false),
        ]
    );
}

#[test]
fn formatting_only_counts_next_to_its_own_marker() {
    // `\n` before `<tool_call>` is formatting, but before the end it's text.
    assert_eq!(
        kinds(&split(&qwen3(), &["Hi", "\n", EOG_TEXT])),
        vec![
            open(Item::Text),
            delta("Hi"),
            delta("\n"),
            CLOSE,
            end(false)
        ]
    );
}

#[test]
fn formatting_has_to_match_exactly() {
    // One newline where the template writes two is the model's.
    let pieces = ["<think>", "Hmm", "</think>", "\n", "Hi"];
    assert_eq!(
        kinds(&split(&qwen3(), &pieces)),
        vec![
            open(Item::Thinking),
            delta("Hmm"),
            CLOSE,
            open(Item::Text),
            delta("\nHi"),
            CLOSE,
            end(true),
        ]
    );
}

/// Only the end of the prompt can leave reasoning open, not an unclosed
/// marker in an earlier turn.
#[test]
fn an_earlier_unclosed_marker_leaves_nothing_open() {
    let prompt = "<|im_start|>assistant\n<think>\nOkay, the user<|im_end|>\n\
                  <|im_start|>user\nWhat is 2+2?<|im_end|>\n<|im_start|>assistant\n";
    assert_eq!(
        kinds(&split_after(&qwen3(), prompt, &["It's 4.", EOG_TEXT])),
        vec![open(Item::Text), delta("It's 4."), CLOSE, end(false)]
    );
}

/// A character the model left unfinished when it was cut off isn't text, so
/// neither the pieces nor the history have it, whether its token has had
/// other text or not.
#[test]
fn a_character_cut_off_by_the_end_is_dropped() {
    let crab = "🦀".as_bytes();
    let tokens: [&[&[u8]]; 2] = [
        &[b"Hi ", &crab[..2]],
        &[&[b"Hi ".as_slice(), &crab[..2]].concat()],
    ];
    for tokens in tokens {
        let resolved = resolve(&qwen3());
        let tools = tools();
        let mut splitter = resolved.splitter(tools, "");
        let mut pieces = Vec::new();
        for bytes in tokens {
            pieces.extend(splitter.push(LlamaToken(1), bytes).0);
        }
        pieces.extend(splitter.finish().0);
        let text: String = pieces
            .iter()
            .flat_map(|p| &p.tokens)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(text, "Hi ");
        assert_eq!(splitter.written(), "Hi ");
    }
}

#[test]
fn a_prompt_can_open_the_reasoning() {
    // Templates that open the reasoning for the model end the prompt inside it.
    let pieces = ["Hmm", "</think>", "Hi"];
    let reasoning = vec![
        open(Item::Thinking),
        delta("Hmm"),
        CLOSE,
        open(Item::Text),
        delta("Hi"),
        CLOSE,
        end(true),
    ];
    let opened = "<|im_start|>assistant\n<think>\n";
    assert_eq!(kinds(&split_after(&qwen35(), opened, &pieces)), reasoning);
    // The formatting after `<think>` is only expected if the prompt left it out.
    let newline_first = ["\nHmm", "</think>", "Hi"];
    assert_eq!(
        kinds(&split_after(
            &qwen35(),
            "<|im_start|>assistant\n<think>",
            &newline_first
        )),
        reasoning
    );
    let mut kept = reasoning.clone();
    kept[1] = delta("\nHmm");
    assert_eq!(kinds(&split_after(&qwen35(), opened, &newline_first)), kept);

    // One that closes it, as when reasoning is turned off, leaves the answer.
    let closed = "<|im_start|>assistant\n<think>\n\n</think>\n\n";
    let (split, warnings) = split_with_warnings(&qwen35(), closed, &pieces);
    assert_eq!(warnings, [Warning::Stray("</think>")]);
    assert_eq!(
        kinds(&split),
        vec![
            open(Item::Text),
            delta("Hmm"),
            delta("</think>"),
            delta("Hi"),
            CLOSE,
            end(true),
        ]
    );
}

#[test]
fn a_reasoning_label_is_not_reasoning() {
    let pieces = ["<|channel>", "thought", "\nHmm", "<channel|>", "Hi"];
    assert_eq!(
        kinds(&split(&gemma4(), &pieces)),
        vec![
            open(Item::Thinking),
            delta("Hmm"),
            CLOSE,
            open(Item::Text),
            delta("Hi"),
            CLOSE,
            end(true),
        ]
    );
    // Without the label, the reasoning is kept whole.
    let pieces = ["<|channel>", "though", "ts", "<channel|>"];
    assert_eq!(
        kinds(&split(&gemma4(), &pieces)),
        vec![open(Item::Thinking), delta("thoughts"), CLOSE, end(true)]
    );
    // A label that's all there is leaves the reasoning empty.
    let pieces = ["<|channel>", "thought\n", "<channel|>"];
    assert_eq!(
        kinds(&split(&gemma4(), &pieces)),
        vec![open(Item::Thinking), CLOSE, end(true)]
    );
}

#[test]
fn the_end_of_generation_ends_the_response() {
    let pieces = ["[TOOL_CALLS]", "get_time[ARGS]{}", EOG_TEXT];
    assert_eq!(
        kinds(&split(&ministral3(), &pieces)),
        vec![open_call("get_time"), delta("{}"), CLOSE, end(false)]
    );
    // A response cut off before it ends all the same, but cut off.
    assert_eq!(
        kinds(&split(&qwen3(), &["Hi"])),
        vec![open(Item::Text), delta("Hi"), CLOSE, end(true)]
    );
}

#[test]
fn a_character_split_across_tokens_arrives_whole() {
    let resolved = resolve(&qwen3());
    let tools = tools();
    let mut splitter = resolved.splitter(tools, "");
    let crab = "🦀".as_bytes();
    assert_eq!(splitter.push(LlamaToken(1), &crab[..2]).0, vec![]);
    let pieces = splitter.push(LlamaToken(2), &crab[2..]).0;
    assert_eq!(kinds(&pieces), vec![open(Item::Text), delta("🦀")]);
    // The token that finishes the character has it as its text.
    let texts: Vec<_> = pieces[1].tokens.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(texts, ["", "🦀"]);
}

#[test]
fn a_character_cut_off_by_a_marker_is_dropped() {
    let resolved = resolve(&qwen3());
    let tools = tools();
    let mut splitter = resolved.splitter(tools, "");
    let crab = "🦀".as_bytes();
    let mut pieces = splitter.push(THINK, b"<think>").0;
    pieces.extend(splitter.push(LlamaToken(1), &crab[..2]).0);
    pieces.extend(splitter.push(UNTHINK, b"</think>").0);
    pieces.extend(splitter.push(LlamaToken(1), &crab[2..]).0);
    assert_eq!(
        with_tokens(&pieces),
        vec![
            (open(Item::Thinking), "<think>".into()),
            (CLOSE, "</think>".into()),
            (open(Item::Text), "".into()),
            (delta("\u{FFFD}\u{FFFD}"), "\u{FFFD}\u{FFFD}".into()),
        ]
    );
}

/// `text` as tokens of at most `size` characters, with each of `format`'s
/// markers a token of its own, as a model's vocabulary has them.
fn tokenize(format: &OutputFormat, text: &str, size: usize) -> Vec<String> {
    let markers: Vec<&str> = specials(format).iter().map(|(marker, _)| *marker).collect();
    let mut tokens = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(marker) = markers.iter().find(|marker| rest.starts_with(**marker)) {
            tokens.push(marker.to_string());
            rest = &rest[marker.len()..];
            continue;
        }
        let next_marker = markers.iter().filter_map(|m| rest.find(m)).min();
        let token: String = rest[..next_marker.unwrap_or(rest.len())]
            .chars()
            .take(size)
            .collect();
        rest = &rest[token.len()..];
        tokens.push(token);
    }
    tokens
}

/// What the history keeps of `output`, a generation after `prompt`.
fn written(format: &OutputFormat, prompt: &str, output: &[String]) -> String {
    let resolved = resolve(format);
    let tools = tools();
    let mut splitter = resolved.splitter(tools, prompt);
    let specials = specials(format);
    for piece in output {
        let token = specials
            .iter()
            .find(|(marker, _)| marker == piece)
            .map(|(_, token)| token.id)
            .unwrap_or(if piece == EOG_TEXT {
                EOG
            } else {
                LlamaToken(1)
            });
        splitter.push(token, piece.as_bytes());
    }
    splitter.finish();
    splitter.written()
}

/// The history keeps exactly what the prompt and the model wrote, however the
/// text is split into tokens: all of it up to the calls, but for the
/// formatting the template writes before them, which it writes again.
#[test]
fn the_history_is_what_was_written() {
    for format in formats() {
        let syntax = format.tool_calls;
        let call = render_response(&format, &calls(&format)[..1]);
        let block = format!("{}{call}", syntax.before_begin);
        // What follows a block without an end is part of it.
        let after_block = if syntax.end.is_some() { "\nDone." } else { "" };
        let mut cases: Vec<(String, String, String)> = vec![
            (
                "".into(),
                "  Hello, 🦀 world.\n  ".into(),
                "  Hello, 🦀 world.\n  ".into(),
            ),
            (
                "".into(),
                format!("Let me check.{block}{after_block}"),
                "Let me check.".into(),
            ),
            (
                "".into(),
                format!("Look:\n\n{block}{after_block}"),
                "Look:\n\n".into(),
            ),
            (
                "".into(),
                format!("{}not a call{}", syntax.begin, syntax.end.unwrap_or("")),
                format!("{}not a call{}", syntax.begin, syntax.end.unwrap_or("")),
            ),
        ];
        if let Some(t) = format.thinking {
            let reasoning = |r: &str| {
                format!(
                    "{}{}{r}{}{}{}",
                    t.begin, t.after_begin, t.before_end, t.end, t.after_end
                )
            };
            cases.extend([
                (
                    "".into(),
                    format!("{}Hi!", reasoning("Hmm.")),
                    format!("{}Hi!", reasoning("Hmm.")),
                ),
                (
                    "".into(),
                    format!("{}Hmm.{}Hi!", t.begin, t.end),
                    format!("{}Hmm.{}Hi!", t.begin, t.end),
                ),
                (
                    "".into(),
                    format!("{}\n\nHi!", reasoning("\nHmm.\n")),
                    format!("{}\n\nHi!", reasoning("\nHmm.\n")),
                ),
                (
                    "".into(),
                    format!("Hi {} there", t.end),
                    format!("Hi {} there", t.end),
                ),
                (
                    format!("<prompt>{}{}", t.begin, t.after_begin),
                    format!("Hmm.{}{}{}Hi!", t.before_end, t.end, t.after_end),
                    format!("{}Hi!", reasoning("Hmm.")),
                ),
            ]);
        }
        for (prompt, output, kept) in &cases {
            for size in [1, 2, 3, 5, 100] {
                for end in ["", EOG_TEXT] {
                    let mut tokens = tokenize(&format, output, size);
                    tokens.extend((!end.is_empty()).then(|| end.to_string()));
                    // A block with no end marker is still open when the
                    // response is cut off, so it's text.
                    let cut_off_in_block = end.is_empty() && syntax.end.is_none();
                    let kept = if cut_off_in_block && output.contains(syntax.begin) {
                        output
                    } else {
                        kept
                    };
                    assert_eq!(
                        &written(&format, prompt, &tokens),
                        kept,
                        "{} wrote {tokens:?} after {prompt:?}",
                        format.name
                    );
                }
            }
        }
    }
}

#[test]
fn stray_end_markers_are_reported_and_kept_as_text() {
    let pieces = ["Hi", "</tool_call>", "</think>", "there"];
    let (split, warnings) = split_with_warnings(&qwen3(), "", &pieces);
    assert_eq!(
        warnings,
        [Warning::Stray("</tool_call>"), Warning::Stray("</think>")]
    );
    assert_eq!(
        with_tokens(&split),
        vec![
            (open(Item::Text), "".into()),
            (delta("Hi"), "Hi".into()),
            (delta("</tool_call>"), "</tool_call>".into()),
            (delta("</think>"), "</think>".into()),
            (delta("there"), "there".into()),
            (CLOSE, "".into()),
            (end(true), "".into()),
        ]
    );
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "after the end of generation")]
fn pushing_after_the_end_is_a_bug() {
    split(&qwen3(), &[EOG_TEXT, "more"]);
}

#[test]
fn without_a_format_everything_but_the_end_is_text() {
    let plain = ModelOutput::plain(&FakeVocab(vec![]));
    let mut splitter = plain.splitter(Vec::new(), "");
    let mut pieces = splitter.push(LlamaToken(1), b"Hi ").0;
    pieces.extend(splitter.push(THINK, b"<think>").0);
    pieces.extend(splitter.push(EOG, EOG_TEXT.as_bytes()).0);
    assert_eq!(
        with_tokens(&pieces),
        vec![
            (open(Item::Text), "".into()),
            (delta("Hi "), "Hi ".into()),
            // Thinking is read literally because the format has no reasoning
            // markers.
            (delta("<think>"), "<think>".into()),
            (CLOSE, "".into()),
            (end(false), EOG_TEXT.into()),
        ]
    );
}

#[test]
fn a_model_without_reasoning_markers_does_not_reason() {
    let syntax = qwen3().tool_calls;
    let text = |id| SpecialToken { id, control: false };
    let vocab = FakeVocab(vec![
        (syntax.begin, text(BEGIN)),
        (syntax.end.unwrap(), text(END)),
    ]);
    let resolved = ResolvedFormat::new(qwen3(), &vocab).unwrap();
    assert_eq!(resolved.thinking, None);
    let tools = tools();
    let mut splitter = resolved.splitter(tools, "<think>\n");
    assert_eq!(
        kinds(&splitter.push(LlamaToken(1), b"Hi").0),
        vec![open(Item::Text), delta("Hi")]
    );
}

// ============================================================================
// Detection
// ============================================================================

#[test]
fn detects_each_format_from_its_own_markers() {
    for format in formats() {
        let template = render_response(&format, &[call("get_time", json!({}))]);
        let detected = detect_from_template(&template).unwrap();
        assert_eq!(detected.name, format.name, "{template:?}");
    }
}

/// A format, and the `(architecture, name)` pairs that should give it.
type MetadataCase<'a> = (Option<OutputFormat>, &'a [(&'a str, &'a str)]);

/// Checks `detect_from_metadata` on each case.
fn assert_detected_from_metadata(cases: &[MetadataCase]) {
    for &(expected, metadata) in cases {
        for &(arch, name) in metadata {
            assert_eq!(
                detect_from_metadata(arch, name).map(|f| f.name),
                expected.map(|f| f.name),
                "architecture {arch:?}, name {name:?}"
            );
        }
    }
}

#[test]
fn tells_qwen35_and_36_from_qwen3_in_metadata() {
    assert_detected_from_metadata(&[
        (
            Some(qwen35()),
            &[
                ("qwen35", "Qwen3.5 2B Instruct"),
                ("qwen35moe", "Qwen3.5 35B A3B"),
                ("qwen36", ""),
                ("", "Qwen3.5-2B-Instruct"),
                ("", "Qwen3.6-30B-A3B"),
                ("", "Qwen 3.5 Coder"),
                ("", "Qwen-3.6 Reasoning"),
            ],
        ),
        (
            Some(qwen3()),
            &[
                ("qwen3", "Qwen3 8B"),
                ("qwen3moe", "Qwen3 30B A3B"),
                ("", "Qwen3-8B-Instruct"),
                ("qwen2", "Qwen2.5 7B Instruct"),
            ],
        ),
    ]);
}

#[test]
fn detects_other_formats_from_metadata() {
    assert_detected_from_metadata(&[
        (Some(lfm2()), &[("lfm2", "LFM2 1.2B"), ("", "LFM2-350M")]),
        (
            Some(function_gemma()),
            &[("gemma3", "functiongemma-270m-it")],
        ),
        (Some(gemma4()), &[("", "gemma-4-e2b-it")]),
        (
            None,
            &[
                ("llama", "Llama 3.1 8B Instruct"),
                ("gemma3", "Gemma 3 4B It"),
                ("", ""),
            ],
        ),
    ]);
}

// ============================================================================
// Against real vocabularies and llguidance
// ============================================================================

fn accepts(model: &llama_cpp_2::model::LlamaModel, grammar: &str, text: &str) -> bool {
    accepts_tokens(model, grammar, &tokens(model, text))
}

/// `text` as the ids llama.cpp's tokenizer gives it, since llguidance's
/// doesn't know every special token and these are the ids the model emits.
fn tokens(model: &llama_cpp_2::model::LlamaModel, text: &str) -> Vec<u32> {
    model
        .vocab()
        .tokenize(text.as_bytes(), false, true)
        .iter()
        .map(|t| t.0 as u32)
        .collect()
}

/// `text` as `tokens` gives it, but with `format`'s block markers as the ids
/// `resolve` gives them.
fn marked_tokens(
    model: &llama_cpp_2::model::LlamaModel,
    format: &OutputFormat,
    text: &str,
) -> Vec<u32> {
    let syntax = format.tool_calls;
    let mut markers = vec![(syntax.begin, BEGIN)];
    markers.extend(syntax.end.map(|end| (end, END)));
    let mut ids = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let next = markers
            .iter()
            .filter_map(|&(marker, id)| rest.find(marker).map(|at| (at, marker, id)))
            .min_by_key(|&(at, ..)| at);
        let Some((at, marker, id)) = next else {
            ids.extend(tokens(model, rest));
            break;
        };
        if at > 0 {
            ids.extend(tokens(model, &rest[..at]));
        }
        ids.push(id.0 as u32);
        rest = &rest[at + marker.len()..];
    }
    ids
}

fn accepts_tokens(model: &llama_cpp_2::model::LlamaModel, grammar: &str, tokens: &[u32]) -> bool {
    use llguidance::toktrie::InferenceCapabilities;
    use llguidance::{api::TopLevelGrammar, Matcher, ParserFactory};

    let tok_env = llama_cpp_2::sampling::LlamaSampler::llguidance_tok_env(model);
    let factory = ParserFactory::new(&tok_env, InferenceCapabilities::default(), &[]).unwrap();
    let grammar = TopLevelGrammar::from_tagged_str("lark", grammar).unwrap();
    let parser = factory
        .create_parser(grammar)
        .unwrap_or_else(|e| panic!("grammar doesn't compile: {e}"));
    let mut matcher = Matcher::new(Ok(parser));
    matcher.try_consume_tokens(tokens).unwrap_or(0) == tokens.len()
        && matcher.is_accepting().unwrap_or(false)
}

/// A block's end is its token, which is the only way the splitter reads it,
/// so the grammar doesn't let the model spell it out as text.
#[test]
fn a_block_ends_only_with_its_token() {
    let model = load_test_vocab();
    let resolved = ResolvedFormat::new(qwen3(), &model).unwrap();
    let grammar = resolved.grammar(&tools()).unwrap();
    let text = render_response(&qwen3(), &calls(&qwen3())[..1]);
    assert!(accepts(&model, &grammar, &text));
    let body = text.strip_suffix("</tool_call>").unwrap();
    let mut spelled = tokens(&model, body);
    spelled.extend(tokens(&model, "</tool_call"));
    spelled.extend(tokens(&model, ">"));
    assert!(!accepts_tokens(&model, &grammar, &spelled));
}

/// Every format's grammar accepts every call it writes, and all of them at
/// once where a response can hold them. The block markers are the ids
/// `resolve` gives them, and the rest is text, so any tokenizer can check it.
#[test]
fn every_grammar_accepts_what_its_format_writes() {
    let model = load_test_vocab();
    for format in formats() {
        let grammar = resolve(&format).grammar(&tools()).unwrap();
        let calls = calls(&format);
        for call in &calls {
            let text = render_response(&format, std::slice::from_ref(call));
            assert!(
                accepts_tokens(&model, &grammar, &marked_tokens(&model, &format, &text)),
                "{} rejected {text:?}\n{grammar}",
                format.name
            );
        }
        let syntax = format.tool_calls;
        if syntax.list.is_some() || syntax.several_blocks {
            let text = render_response(&format, &calls);
            assert!(
                accepts_tokens(&model, &grammar, &marked_tokens(&model, &format, &text)),
                "{} rejected {text:?}\n{grammar}",
                format.name
            );
        }
    }
}

/// FunctionGemma repeated its call until the context ran out when the grammar
/// let it, so a format without several blocks ends the response after one.
#[test]
fn only_formats_with_several_blocks_allow_another() {
    let model = load_test_vocab();
    for format in formats() {
        let grammar = resolve(&format).grammar(&tools()).unwrap();
        let block = render_response(&format, &calls(&format)[..1]);
        let twice = format!("{block}{block}");
        assert_eq!(
            accepts_tokens(&model, &grammar, &marked_tokens(&model, &format, &twice)),
            format.tool_calls.several_blocks,
            "{} on {twice:?}\n{grammar}",
            format.name
        );
    }
}

#[test]
fn grammars_hold_the_model_to_the_schema() {
    let model = load_test_vocab();
    let wrong = [
        call("get_weather", json!({})),
        call(
            "get_weather",
            json!({ "location": "Paris", "unit": "kelvin" }),
        ),
        call("calculate", json!({ "x": 1, "y": 1.5 })),
        call("no_such_tool", json!({})),
    ];
    for format in formats() {
        let resolved = resolve(&format);
        let grammar = resolved.grammar(&tools()).unwrap();
        for call in &wrong {
            // `no_such_tool` has no schema, so render it as if it took none.
            let text = if call.name == "no_such_tool" {
                render_response(&format, &[self::call("get_time", json!({}))])
                    .replace("get_time", "no_such_tool")
            } else {
                render_response(&format, std::slice::from_ref(call))
            };
            assert!(
                !accepts_tokens(&model, &grammar, &marked_tokens(&model, &format, &text)),
                "{} accepted {text:?}",
                format.name
            );
        }
    }
}

#[test]
fn checks_markers_against_the_vocabulary() {
    let model = load_test_vocab();
    let qwen3 = ResolvedFormat::new(qwen3(), &model).unwrap();
    let id = |text| model.special_token(text).unwrap().id;
    assert_eq!(qwen3.tool_calls.begin, id("<tool_call>"));
    let thinking = ThinkingTokens {
        begin: id("<think>"),
        end: id("</think>"),
    };
    assert_eq!(qwen3.thinking, Some(thinking));
    assert!(qwen3.end_of_generation.contains(&id("<|im_end|>")));
    // Qwen's markers are user-defined tokens, which have text.
    assert!(qwen3.control.is_empty());

    assert!(matches!(
        ResolvedFormat::new(gemma4(), &model),
        Err(FormatError::NotSpecial {
            marker: "<|tool_call>",
            ..
        })
    ));
}

#[test]
fn qwen_grammars_accept_calls_through_the_real_vocabulary() {
    let model = load_test_vocab();
    for format in [&qwen3(), &qwen35()] {
        let resolved = ResolvedFormat::new(*format, &model).unwrap();
        let grammar = resolved.grammar(&tools()).unwrap();
        let text = render_response(format, &calls(format));
        assert!(
            accepts(&model, &grammar, &text),
            "{} rejected {text:?}\n{grammar}",
            format.name
        );
    }
}

#[test]
fn gemma4_grammar_accepts_calls_through_the_real_vocabulary() {
    let Some(model) = load_gemma4_vocab() else {
        eprintln!("skipping: set GEMMA4_MODEL to a Gemma4 GGUF to run this test");
        return;
    };
    let resolved = ResolvedFormat::new(gemma4(), &model).unwrap();
    let id = |text| model.special_token(text).unwrap().id;
    assert_eq!(
        resolved.thinking,
        Some(ThinkingTokens {
            begin: id("<|channel>"),
            end: id("<channel|>"),
        })
    );
    // Gemma4 ends generation to wait for a tool's result.
    assert!(resolved.end_of_generation.contains(&id("<|tool_response>")));

    let grammar = resolved.grammar(&tools()).unwrap();
    let text = render_response(&gemma4(), &calls(&gemma4()));
    assert!(
        accepts(&model, &grammar, &text),
        "rejected {text:?}\n{grammar}"
    );
}

/// What Qwen3's own template writes around its markers is what `Qwen3`
/// declares as formatting.
#[test]
fn qwen3_formatting_is_what_its_template_writes() {
    use crate::chat::Message;
    use crate::content::MessageContent;
    use crate::template::{select_template, ChatTemplateContext};

    let model = load_test_vocab();
    let template = select_template(&model, true).unwrap();
    let get_time = call("get_time", json!({}));
    let messages = [
        Message::new_user("Hi"),
        Message::Assistant {
            content: MessageContent::text("<think>\nREASONING\n</think>\n\nCONTENT"),
            tool_calls: Some(vec![get_time.clone(), get_time]),
        },
    ];
    let context = ChatTemplateContext::new(Default::default(), Some(tools()));
    let rendered = template.render(&messages, &context, true).unwrap();

    let thinking = qwen3().thinking.unwrap();
    let syntax = qwen3().tool_calls;
    for expected in [
        format!(
            "{}{}REASONING{}{}{}CONTENT",
            thinking.begin,
            thinking.after_begin,
            thinking.before_end,
            thinking.end,
            thinking.after_end
        ),
        format!("CONTENT{}{}", syntax.before_begin, syntax.begin),
    ] {
        assert!(
            rendered.contains(&expected),
            "{expected:?} not in {rendered:?}"
        );
    }
    // Between blocks there's `after_end`, and maybe `before_begin` after it.
    // The system prompt shows the format too, so this looks past the content.
    let (_, calls) = rendered.split_once("CONTENT").unwrap();
    let between = calls
        .split(syntax.end.unwrap())
        .nth(1)
        .and_then(|rest| rest.split(syntax.begin).next())
        .unwrap();
    assert!(
        between == syntax.after_end
            || between == format!("{}{}", syntax.after_end, syntax.before_begin),
        "{between:?} between blocks in {rendered:?}"
    );
}

/// A marker, the Qwen3 token standing in for it, and that token.
type StandIn = (&'static str, &'static str, SpecialToken);

/// Markers the vocabulary has as control tokens are named by id. Qwen3's
/// `<|im_start|>` and `<|vision_start|>` stand in for another model's markers,
/// since neither ends generation.
#[test]
fn control_token_markers_are_named_by_id() {
    let model = load_test_vocab();
    let im_start = model.special_token("<|im_start|>").unwrap();
    let vision = model.special_token("<|vision_start|>").unwrap();
    assert!(im_start.control && vision.control);

    let cases: [(&OutputFormat, &[StandIn]); 2] = [
        (
            &ministral3(),
            &[
                ("[TOOL_CALLS]", "<|im_start|>", im_start),
                ("[ARGS]", "<|vision_start|>", vision),
            ],
        ),
        (
            &function_gemma(),
            &[
                ("<start_function_call>", "<|im_start|>", im_start),
                ("<escape>", "<|vision_start|>", vision),
                (
                    "<end_function_call>",
                    "<end_function_call>",
                    SpecialToken {
                        id: END,
                        control: false,
                    },
                ),
            ],
        ),
    ];
    for (format, markers) in cases {
        let vocab = FakeVocab(
            markers
                .iter()
                .map(|&(marker, _, token)| (marker, token))
                .collect(),
        );
        let resolved = ResolvedFormat::new(*format, &vocab).unwrap();
        let grammar = resolved.grammar(&tools()).unwrap();
        assert!(
            grammar.contains(&format!("<[{}]>", vision.id.0)),
            "{grammar}"
        );

        for call in calls(format) {
            let mut text = render_response(format, &[call]);
            for (marker, stand_in, _) in markers.iter() {
                text = text.replace(marker, stand_in);
            }
            assert!(
                accepts_tokens(&model, &grammar, &marked_tokens(&model, format, &text)),
                "{} rejected {text:?}\n{grammar}",
                format.name
            );
        }
    }
}

#[test]
fn lark_names_and_literals_are_escaped() {
    assert_eq!(escape_lark_string("a\"b\\c"), "a\\\"b\\\\c");
    assert_eq!(escape_lark_string("l1\nl2\tx\r"), "l1\\nl2\\tx\\r");
    // A backslash then `n` isn't a newline.
    assert_eq!(escape_lark_string("a\\nb"), "a\\\\nb");
    assert_eq!(sanitize_lark("activeForm"), "activeform");
    assert_eq!(sanitize_lark("blocked-by"), "blocked_by");
}

/// Values that end where a format's own markers could begin, in arguments
/// whose names Lark can't take as they are.
#[test]
fn awkward_names_and_values_make_working_grammars() {
    let model = load_test_vocab();
    let tools = tools();
    let call = call(
        "set_task",
        json!({ "activeForm": "sunny\n", "mode": "with\nnewline" }),
    );
    for format in formats() {
        let resolved = resolve(&format);
        let grammar = resolved.grammar(&tools).unwrap();
        let text = render_response(&format, std::slice::from_ref(&call));
        assert!(
            accepts_tokens(&model, &grammar, &marked_tokens(&model, &format, &text)),
            "{} rejected {text:?}\n{grammar}",
            format.name
        );
        let text = render_tool_calls(&format, std::slice::from_ref(&call));
        assert_eq!(
            resolved.parse_tool_calls(&text, &tools),
            Ok(vec![call.clone()]),
            "{} failed on {text:?}",
            format.name
        );
    }
}

/// llguidance ignores schema keywords it doesn't implement, like
/// `uniqueItems`, and still enforces the ones it does, like `maximum`.
#[test]
fn unimplemented_schema_keywords_are_ignored_not_fatal() {
    let schema = json!({
        "type": "object",
        "properties": {
            "tags": { "type": "array", "items": { "type": "string" }, "uniqueItems": true },
            "score": { "type": "integer", "minimum": 0, "maximum": 5 },
        },
        "required": ["score"],
    });
    let embedded = json_schema_for_llguidance(&schema);
    assert!(embedded.contains("\"lenient\":true"), "{embedded}");
    assert!(embedded.contains("uniqueItems"), "{embedded}");

    let model = load_test_vocab();
    let grammar = resolve(&qwen3()).grammar(&[tool("rate", schema)]).unwrap();
    let rate = |score| render_response(&qwen3(), &[call("rate", json!({ "score": score }))]);
    assert!(
        accepts_tokens(&model, &grammar, &marked_tokens(&model, &qwen3(), &rate(3))),
        "{grammar}"
    );
    assert!(
        !accepts_tokens(&model, &grammar, &marked_tokens(&model, &qwen3(), &rate(9))),
        "{grammar}"
    );
}
