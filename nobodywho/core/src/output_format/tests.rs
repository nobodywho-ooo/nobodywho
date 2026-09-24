use super::grammar::{escape_lark_string, json_schema_for_llguidance, properties, sanitize_lark};
use super::*;
use crate::tool_calling::{Tool, ToolCall};
use llama_cpp_2::model::params::LlamaModelParams;
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
fn specials(format: &dyn OutputFormat) -> Vec<(&'static str, SpecialToken)> {
    let text = |id| SpecialToken { id, control: false };
    let syntax = format.tool_calls();
    let mut specials = vec![(syntax.begin, text(BEGIN))];
    specials.extend(syntax.end.map(|end| (end, text(END))));
    if let Some(thinking) = format.thinking() {
        specials.extend([(thinking.begin, text(THINK)), (thinking.end, text(UNTHINK))]);
    }
    specials
}

/// `format` over a vocabulary that has its markers as text.
fn resolve(format: &'static dyn OutputFormat) -> ResolvedFormat {
    ResolvedFormat::new(format, &FakeVocab(specials(format))).unwrap()
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
fn calls(format: &dyn OutputFormat) -> Vec<ToolCall> {
    let mut calls = vec![
        call("get_weather", json!({ "location": "Paris" })),
        call(
            "get_weather",
            json!({ "location": "count < 5, \"quoted\"\nnext line", "unit": "celsius" }),
        ),
        call("calculate", json!({ "exact": false, "x": 1.5, "y": -2 })),
        call("get_time", json!({})),
    ];
    let nests = match format.tool_calls().call {
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

fn render_call(format: &dyn OutputFormat, call: &ToolCall) -> String {
    let syntax = match format.tool_calls().call {
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
fn render_tool_calls(format: &dyn OutputFormat, calls: &[ToolCall]) -> String {
    let calls: Vec<_> = calls.iter().map(|c| render_call(format, c)).collect();
    match format.tool_calls().list {
        Some(list) => format!("{}{}{}", list.open, calls.join(list.separator), list.close),
        None => {
            assert_eq!(calls.len(), 1, "{format:?} has one call per block");
            calls[0].clone()
        }
    }
}

/// A full response making `calls`, markers included.
fn render_response(format: &dyn OutputFormat, calls: &[ToolCall]) -> String {
    let syntax = format.tool_calls();
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
    for &format in FORMATS {
        let resolved = resolve(format);
        for call in calls(format) {
            let text = render_tool_calls(format, std::slice::from_ref(&call));
            assert_eq!(
                resolved.parse_tool_calls(&text, &tools()),
                Ok(vec![call]),
                "{format:?} failed on {text:?}"
            );
        }
    }
}

#[test]
fn reads_what_the_old_handlers_were_tested_on() {
    let cases: &[(&'static dyn OutputFormat, &str, Vec<ToolCall>)] = &[
        (
            &Qwen3,
            r#"{"name": "tool1", "arguments": {"a": 1}}"#,
            vec![call("tool1", json!({ "a": 1 }))],
        ),
        (
            &Qwen35,
            "\n<function=get_weather>\n<parameter=location>\nsunny\n\n</parameter>\n</function>\n",
            vec![call("get_weather", json!({ "location": "sunny\n" }))],
        ),
        (
            &FunctionGemma,
            "call:calculate{x:<escape>10<escape>, y:<escape>20<escape>, op:<escape>add<escape>}",
            vec![call("calculate", json!({ "x": 10, "y": 20, "op": "add" }))],
        ),
        (
            &FunctionGemma,
            "call:write_file{content:<escape>line1\nline2<escape>}",
            vec![call("write_file", json!({ "content": "line1\nline2" }))],
        ),
        (
            &Gemma4,
            "call:search{query:<|\"|>rust lang<|\"|>,limit:10,exact:false}",
            vec![call(
                "search",
                json!({ "query": "rust lang", "limit": 10, "exact": false }),
            )],
        ),
        (
            &Ministral3,
            r#"sparklify[ARGS]{"text": "JULEMAND"}"#,
            vec![call("sparklify", json!({ "text": "JULEMAND" }))],
        ),
        (
            &Lfm2,
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
            resolve(*format).parse_tool_calls(text, &tools()),
            Ok(expected.clone()),
            "{format:?} failed on {text:?}"
        );
    }
}

#[test]
fn a_json_call_can_name_its_arguments_first() {
    let text = r#"{"arguments": {"location": "Oslo"}, "name": "get_weather"}"#;
    assert_eq!(
        resolve(&Qwen3).parse_tool_calls(text, &tools()),
        Ok(vec![call("get_weather", json!({ "location": "Oslo" }))])
    );
}

#[test]
fn values_are_read_by_their_schema_type() {
    // A string that looks like a number stays a string when the schema says so.
    let text = "\n<function=get_weather>\n<parameter=location>\n123\n</parameter>\n</function>\n";
    assert_eq!(
        resolve(&Qwen35).parse_tool_calls(text, &tools()),
        Ok(vec![call("get_weather", json!({ "location": "123" }))])
    );
}

#[test]
fn reads_whitespace_the_template_would_not_have_written() {
    let text = r#"{"name":"get_time","arguments":{}}"#;
    assert_eq!(
        resolve(&Qwen3).parse_tool_calls(text, &tools()),
        Ok(vec![call("get_time", json!({}))])
    );
    let text = "call:calculate{x:<escape>1<escape>,y:<escape>2<escape>}";
    assert_eq!(
        resolve(&FunctionGemma).parse_tool_calls(text, &tools()),
        Ok(vec![call("calculate", json!({ "x": 1, "y": 2 }))])
    );
}

#[test]
fn says_where_a_block_stops_making_sense() {
    let error = resolve(&Gemma4)
        .parse_tool_calls("call:get_time{", &tools())
        .unwrap_err();
    assert_eq!(error.at, "call:get_time{".len());

    let error = resolve(&Qwen3)
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

/// Feeds `pieces` through a splitter for a response to `prompt`. Markers get
/// their tokens, and anything else is token 1.
fn split_after(format: &'static dyn OutputFormat, prompt: &str, pieces: &[&str]) -> Vec<Piece> {
    let resolved = resolve(format);
    let tools = tools();
    let mut splitter = resolved.splitter(&tools, prompt);
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
    let mut out: Vec<Piece> = pieces
        .iter()
        .flat_map(|&piece| splitter.push(token(piece), piece.as_bytes()))
        .collect();
    out.extend(splitter.finish());
    out
}

fn split(format: &'static dyn OutputFormat, pieces: &[&str]) -> Vec<Piece> {
    split_after(format, "", pieces)
}

#[test]
fn splits_text_from_calls() {
    let pieces = [
        "Let me ",
        "check.",
        "<tool_call>",
        "\n{\"name\": \"get_time\", ",
        "\"arguments\": {}}\n",
        "</tool_call>",
        "\n",
    ];
    assert_eq!(
        split(&Qwen3, &pieces),
        vec![
            Piece::Text("Let me ".into()),
            Piece::Text("check.".into()),
            Piece::Calls(vec![call("get_time", json!({}))]),
            Piece::Text("\n".into()),
        ]
    );
}

#[test]
fn markers_spelled_in_text_are_text() {
    // The characters of `<tool_call>` arriving as ordinary tokens.
    let pieces = ["use ", "<tool", "_call>", " to call tools"];
    assert_eq!(split(&Qwen3, &pieces).len(), 4);
    assert!(split(&Qwen3, &pieces)
        .iter()
        .all(|piece| matches!(piece, Piece::Text(_))));
}

#[test]
fn a_block_without_an_end_runs_to_the_next() {
    let pieces = [
        "[TOOL_CALLS]",
        "get_time[ARGS]{}",
        "[TOOL_CALLS]",
        "get_weather[ARGS]",
        "{\"location\": \"Oslo\"}",
    ];
    assert_eq!(
        split(&Ministral3, &pieces),
        vec![
            Piece::Calls(vec![call("get_time", json!({}))]),
            Piece::Calls(vec![call("get_weather", json!({ "location": "Oslo" }))]),
        ]
    );
}

#[test]
fn a_broken_block_comes_back_as_its_text() {
    let pieces = ["<|tool_call>", "call:get_time(", "<tool_call|>"];
    let pieces: [Piece; 1] = split(&Gemma4, &pieces).try_into().unwrap();
    let [Piece::Malformed { text, .. }] = pieces else {
        panic!("expected a malformed block, got {pieces:?}");
    };
    assert_eq!(text, "<|tool_call>call:get_time(<tool_call|>");
}

#[test]
fn a_cut_off_block_is_still_read() {
    let pieces = ["<|tool_call_start|>", "[get_time()]"];
    assert_eq!(
        split(&Lfm2, &pieces),
        vec![Piece::Calls(vec![call("get_time", json!({}))])]
    );
}

#[test]
fn separates_reasoning_from_the_answer() {
    let pieces = ["<think>", "\nHmm", ".\n", "</think>", "\n\nHi", EOG_TEXT];
    assert_eq!(
        split(&Qwen3, &pieces),
        vec![
            Piece::Thinking("\nHmm".into()),
            Piece::Thinking(".\n".into()),
            Piece::Text("\n\nHi".into()),
            Piece::End,
        ]
    );
}

#[test]
fn a_prompt_can_open_the_reasoning() {
    // Templates that open the reasoning for the model end the prompt inside it.
    let pieces = ["Hmm", "</think>", "Hi"];
    let reasoning = vec![Piece::Thinking("Hmm".into()), Piece::Text("Hi".into())];
    assert_eq!(
        split_after(&Qwen35, "<|im_start|>assistant\n<think>\n", &pieces),
        reasoning
    );

    // One that closes it, as when reasoning is turned off, leaves the answer.
    let closed = "<|im_start|>assistant\n<think>\n\n</think>\n\n";
    assert_eq!(
        split_after(&Qwen35, closed, &pieces),
        vec![
            Piece::Text("Hmm".into()),
            Piece::Stray("</think>"),
            Piece::Text("Hi".into()),
        ]
    );
}

#[test]
fn a_reasoning_label_is_not_reasoning() {
    let pieces = ["<|channel>", "thought", "\nHmm", "<channel|>", "Hi"];
    assert_eq!(
        split(&Gemma4, &pieces),
        vec![Piece::Thinking("Hmm".into()), Piece::Text("Hi".into())]
    );
    // Without the label, the reasoning is kept whole.
    let pieces = ["<|channel>", "though", "ts", "<channel|>"];
    assert_eq!(
        split(&Gemma4, &pieces),
        vec![Piece::Thinking("thoughts".into())]
    );
    // Nor is a label that's all the reasoning there is.
    let pieces = ["<|channel>", "thought\n", "<channel|>"];
    assert_eq!(split(&Gemma4, &pieces), vec![]);
}

#[test]
fn the_end_of_generation_ends_the_response() {
    let pieces = ["[TOOL_CALLS]", "get_time[ARGS]{}", EOG_TEXT];
    assert_eq!(
        split(&Ministral3, &pieces),
        vec![Piece::Calls(vec![call("get_time", json!({}))]), Piece::End]
    );
    // A response cut off before it doesn't end.
    assert_eq!(split(&Qwen3, &["Hi"]), vec![Piece::Text("Hi".into())]);
}

#[test]
fn a_character_split_across_tokens_arrives_whole() {
    let resolved = resolve(&Qwen3);
    let tools = tools();
    let mut splitter = resolved.splitter(&tools, "");
    let crab = "🦀".as_bytes();
    assert_eq!(splitter.push(LlamaToken(1), &crab[..2]), vec![]);
    assert_eq!(
        splitter.push(LlamaToken(1), &crab[2..]),
        vec![Piece::Text("🦀".into())]
    );
}

#[test]
fn a_character_cut_off_by_a_marker_stays_where_it_started() {
    let resolved = resolve(&Qwen3);
    let tools = tools();
    let mut splitter = resolved.splitter(&tools, "");
    let crab = "🦀".as_bytes();
    let mut pieces = splitter.push(THINK, b"<think>");
    pieces.extend(splitter.push(LlamaToken(1), &crab[..2]));
    pieces.extend(splitter.push(UNTHINK, b"</think>"));
    pieces.extend(splitter.push(LlamaToken(1), &crab[2..]));
    assert_eq!(
        pieces,
        vec![
            Piece::Thinking("\u{FFFD}".into()),
            Piece::Text("\u{FFFD}\u{FFFD}".into()),
        ]
    );
}

#[test]
fn stray_end_markers_are_reported() {
    let pieces = ["Hi", "</tool_call>", "</think>", "there"];
    assert_eq!(
        split(&Qwen3, &pieces),
        vec![
            Piece::Text("Hi".into()),
            Piece::Stray("</tool_call>"),
            Piece::Stray("</think>"),
            Piece::Text("there".into()),
        ]
    );
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "after the end of generation")]
fn pushing_after_the_end_is_a_bug() {
    split(&Qwen3, &[EOG_TEXT, "more"]);
}

#[test]
fn a_model_without_reasoning_markers_does_not_reason() {
    let syntax = Qwen3.tool_calls();
    let text = |id| SpecialToken { id, control: false };
    let vocab = FakeVocab(vec![
        (syntax.begin, text(BEGIN)),
        (syntax.end.unwrap(), text(END)),
    ]);
    let resolved = ResolvedFormat::new(&Qwen3, &vocab).unwrap();
    assert_eq!(resolved.thinking, None);
    let tools = tools();
    let mut splitter = resolved.splitter(&tools, "<think>\n");
    assert_eq!(
        splitter.push(LlamaToken(1), b"Hi"),
        vec![Piece::Text("Hi".into())]
    );
}

// ============================================================================
// Detection
// ============================================================================

#[test]
fn detects_each_format_from_its_own_markers() {
    for &format in FORMATS {
        let template = render_response(format, &[call("get_time", json!({}))]);
        let detected = FORMATS.iter().find(|f| f.detect(&template)).unwrap();
        assert_eq!(
            format!("{detected:?}"),
            format!("{format:?}"),
            "{template:?}"
        );
    }
}

#[test]
fn tells_qwen35_and_36_from_qwen3_in_metadata() {
    for text in [
        "qwen3.5-2b-instruct",
        "qwen3.6-30b-a3b",
        "qwen 3.5 coder",
        "qwen-3.6 reasoning",
        "qwen35moe",
        "qwen36",
    ] {
        assert!(is_qwen35_36(text), "{text}");
    }
    assert!(!is_qwen35_36("qwen3-8b-instruct"));
    assert!(!is_qwen35_36("qwen3"));
}

// ============================================================================
// Against real vocabularies and llguidance
// ============================================================================

fn load_vocab(path: &str) -> llama_cpp_2::model::LlamaModel {
    let params = LlamaModelParams::default().with_vocab_only(true);
    llama_cpp_2::model::LlamaModel::load_from_file(&crate::llm::LLAMA_BACKEND, path, &params)
        .unwrap_or_else(|e| panic!("failed to load vocabulary from {path}: {e}"))
}

fn qwen3_vocab() -> llama_cpp_2::model::LlamaModel {
    load_vocab(&std::env::var("TEST_MODEL").unwrap_or_else(|_| "model.gguf".to_string()))
}

/// Needs `GEMMA4_MODEL`, which may be a vocabulary-only GGUF.
fn gemma4_vocab() -> Option<llama_cpp_2::model::LlamaModel> {
    Some(load_vocab(&std::env::var("GEMMA4_MODEL").ok()?))
}

fn accepts(model: &llama_cpp_2::model::LlamaModel, grammar: &str, text: &str) -> bool {
    use llguidance::toktrie::InferenceCapabilities;
    use llguidance::{api::TopLevelGrammar, Matcher, ParserFactory};

    let tok_env = llama_cpp_2::sampling::LlamaSampler::llguidance_tok_env(model);
    let factory = ParserFactory::new(&tok_env, InferenceCapabilities::default(), &[]).unwrap();
    let grammar = TopLevelGrammar::from_tagged_str("lark", grammar).unwrap();
    let parser = factory
        .create_parser(grammar)
        .unwrap_or_else(|e| panic!("grammar doesn't compile: {e}"));
    let mut matcher = Matcher::new(Ok(parser));
    // llama.cpp's tokenizer, since llguidance's doesn't know every special
    // token and these are the ids the model emits.
    let tokens: Vec<u32> = model
        .str_to_token(text, llama_cpp_2::model::AddBos::Never)
        .unwrap()
        .iter()
        .map(|t| t.0 as u32)
        .collect();
    matcher.try_consume_tokens(&tokens).unwrap_or(0) == tokens.len()
        && matcher.is_accepting().unwrap_or(false)
}

/// Every format's grammar accepts every call it writes, and all of them at
/// once. Markers are text here, so any tokenizer can check them.
#[test]
fn every_grammar_accepts_what_its_format_writes() {
    let model = qwen3_vocab();
    for &format in FORMATS {
        let grammar = resolve(format).grammar(&tools()).unwrap();
        let calls = calls(format);
        for call in &calls {
            let text = render_response(format, std::slice::from_ref(call));
            assert!(
                accepts(&model, &grammar, &text),
                "{format:?} rejected {text:?}\n{grammar}"
            );
        }
        let text = render_response(format, &calls);
        assert!(
            accepts(&model, &grammar, &text),
            "{format:?} rejected {text:?}\n{grammar}"
        );
    }
}

#[test]
fn grammars_hold_the_model_to_the_schema() {
    let model = qwen3_vocab();
    let wrong = [
        call("get_weather", json!({})),
        call(
            "get_weather",
            json!({ "location": "Paris", "unit": "kelvin" }),
        ),
        call("calculate", json!({ "x": 1, "y": 1.5 })),
        call("no_such_tool", json!({})),
    ];
    for &format in FORMATS {
        let resolved = resolve(format);
        let grammar = resolved.grammar(&tools()).unwrap();
        for call in &wrong {
            // `no_such_tool` has no schema, so render it as if it took none.
            let text = if call.name == "no_such_tool" {
                render_response(format, &[self::call("get_time", json!({}))])
                    .replace("get_time", "no_such_tool")
            } else {
                render_response(format, std::slice::from_ref(call))
            };
            assert!(
                !accepts(&model, &grammar, &text),
                "{format:?} accepted {text:?}"
            );
        }
    }
}

#[test]
fn checks_markers_against_the_vocabulary() {
    let model = qwen3_vocab();
    let qwen3 = ResolvedFormat::new(&Qwen3, &model).unwrap();
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
        ResolvedFormat::new(&Gemma4, &model),
        Err(FormatError::NotSpecial {
            marker: "<|tool_call>",
            ..
        })
    ));
}

#[test]
fn qwen_grammars_accept_calls_through_the_real_vocabulary() {
    let model = qwen3_vocab();
    for format in [&Qwen3 as &'static dyn OutputFormat, &Qwen35] {
        let resolved = ResolvedFormat::new(format, &model).unwrap();
        let grammar = resolved.grammar(&tools()).unwrap();
        let text = render_response(format, &calls(format));
        assert!(
            accepts(&model, &grammar, &text),
            "{format:?} rejected {text:?}\n{grammar}"
        );
    }
}

#[test]
fn gemma4_grammar_accepts_calls_through_the_real_vocabulary() {
    let Some(model) = gemma4_vocab() else {
        eprintln!("skipping: set GEMMA4_MODEL to a Gemma4 GGUF to run this test");
        return;
    };
    let resolved = ResolvedFormat::new(&Gemma4, &model).unwrap();
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
    let text = render_response(&Gemma4, &calls(&Gemma4));
    assert!(
        accepts(&model, &grammar, &text),
        "rejected {text:?}\n{grammar}"
    );
}

/// A marker, the Qwen3 token standing in for it, and that token.
type StandIn = (&'static str, &'static str, SpecialToken);

/// Markers the vocabulary has as control tokens are named by id. Qwen3's
/// `<|im_start|>` and `<|vision_start|>` stand in for another model's markers,
/// since neither ends generation.
#[test]
fn control_token_markers_are_named_by_id() {
    let model = qwen3_vocab();
    let im_start = model.special_token("<|im_start|>").unwrap();
    let vision = model.special_token("<|vision_start|>").unwrap();
    assert!(im_start.control && vision.control);

    let cases: [(&'static dyn OutputFormat, &[StandIn]); 2] = [
        (
            &Ministral3,
            &[
                ("[TOOL_CALLS]", "<|im_start|>", im_start),
                ("[ARGS]", "<|vision_start|>", vision),
            ],
        ),
        (
            &FunctionGemma,
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
        let resolved = ResolvedFormat::new(format, &vocab).unwrap();
        let grammar = resolved.grammar(&tools()).unwrap();
        assert!(
            grammar.contains(&format!("<[{}]>", vision.id.0)),
            "{grammar}"
        );

        let mut text = render_response(format, &calls(format));
        for (marker, stand_in, _) in markers.iter() {
            text = text.replace(marker, stand_in);
        }
        assert!(
            accepts(&model, &grammar, &text),
            "{format:?} rejected {text:?}\n{grammar}"
        );
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
    let model = qwen3_vocab();
    let tools = tools();
    let call = call(
        "set_task",
        json!({ "activeForm": "sunny\n", "mode": "with\nnewline" }),
    );
    for &format in FORMATS {
        let resolved = resolve(format);
        let grammar = resolved.grammar(&tools).unwrap();
        let text = render_response(format, std::slice::from_ref(&call));
        assert!(
            accepts(&model, &grammar, &text),
            "{format:?} rejected {text:?}\n{grammar}"
        );
        let text = render_tool_calls(format, std::slice::from_ref(&call));
        assert_eq!(
            resolved.parse_tool_calls(&text, &tools),
            Ok(vec![call.clone()]),
            "{format:?} failed on {text:?}"
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

    let model = qwen3_vocab();
    let grammar = resolve(&Qwen3).grammar(&[tool("rate", schema)]).unwrap();
    let rate = |score| render_response(&Qwen3, &[call("rate", json!({ "score": score }))]);
    assert!(accepts(&model, &grammar, &rate(3)), "{grammar}");
    assert!(!accepts(&model, &grammar, &rate(9)), "{grammar}");
}
