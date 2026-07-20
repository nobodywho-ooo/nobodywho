use super::{Tool, ToolCall, ToolFormatError, ToolFormatHandler};
use nom::{
    branch::alt as nom_alt,
    bytes::complete::{tag, take_until, take_while1},
    combinator::{map, value},
    multi::{many1, separated_list0},
    number::complete::recognize_float,
    sequence::{delimited, separated_pair},
    IResult, Parser,
};
use serde_json::Value;
use tracing::debug;

// ============================================================================
// Token constants
// ============================================================================

const QUOTE_TOKEN: &str = "<|\"|>";
const BEGIN_TOKEN: &str = "<|tool_call>";
const END_TOKEN: &str = "<tool_call|>";

// ============================================================================
// nom parsers for Gemma4 value format
// ============================================================================

/// Parse a Gemma4 string: <|"|>content<|"|>
fn gemma4_string(input: &str) -> IResult<&str, Value> {
    map(
        delimited(tag(QUOTE_TOKEN), take_until(QUOTE_TOKEN), tag(QUOTE_TOKEN)),
        |s: &str| Value::String(s.to_string()),
    )
    .parse(input)
}

/// Parse a Gemma4 boolean: true or false
fn gemma4_bool(input: &str) -> IResult<&str, Value> {
    nom_alt((
        value(Value::Bool(true), tag("true")),
        value(Value::Bool(false), tag("false")),
    ))
    .parse(input)
}

/// Parse a Gemma4 null
fn gemma4_null(input: &str) -> IResult<&str, Value> {
    value(Value::Null, tag("null")).parse(input)
}

/// Parse a Gemma4 number (integer or float)
fn gemma4_number(input: &str) -> IResult<&str, Value> {
    map(recognize_float, |s: &str| {
        if let Ok(i) = s.parse::<i64>() {
            Value::Number(i.into())
        } else if let Some(n) = s.parse::<f64>().ok().and_then(serde_json::Number::from_f64) {
            Value::Number(n)
        } else {
            Value::String(s.to_string())
        }
    })
    .parse(input)
}

/// Parse a bare key (alphanumeric + underscore)
fn gemma4_key(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_alphanumeric() || c == '_').parse(input)
}

/// Parse a key:value pair
fn key_value_pair(input: &str) -> IResult<&str, (String, Value)> {
    map(
        separated_pair(gemma4_key, tag(":"), gemma4_value),
        |(k, v)| (k.to_string(), v),
    )
    .parse(input)
}

/// Parse a Gemma4 object: {key:value,key:value,...}
fn gemma4_object(input: &str) -> IResult<&str, Value> {
    map(
        delimited(
            tag("{"),
            separated_list0(tag(","), key_value_pair),
            tag("}"),
        ),
        |pairs| Value::Object(pairs.into_iter().collect()),
    )
    .parse(input)
}

/// Parse a Gemma4 array: [value,value,...]
fn gemma4_array(input: &str) -> IResult<&str, Value> {
    map(
        delimited(tag("["), separated_list0(tag(","), gemma4_value), tag("]")),
        Value::Array,
    )
    .parse(input)
}

/// Parse any Gemma4 value
fn gemma4_value(input: &str) -> IResult<&str, Value> {
    // Order matters: bool/null before number (avoid "true" parsed as ident),
    // string before object (both could start with <)
    nom_alt((
        gemma4_string,
        gemma4_bool,
        gemma4_null,
        gemma4_object,
        gemma4_array,
        gemma4_number,
    ))
    .parse(input)
}

/// Parse a single tool call: <|tool_call>call:name{args}<tool_call|>
fn single_tool_call(input: &str) -> IResult<&str, ToolCall> {
    let (input, _) = tag(BEGIN_TOKEN)(input)?;
    let (input, _) = tag("call:")(input)?;
    let (input, name) = gemma4_key(input)?;
    let (input, args) = gemma4_object(input)?;
    let (input, _) = tag(END_TOKEN)(input)?;
    Ok((
        input,
        ToolCall {
            name: name.to_string(),
            arguments: args,
        },
    ))
}

// ============================================================================
// Grammar generation (Lark)
// ============================================================================

// Gemma4's delimiters are registered in the tokenizer with the angle brackets
// as part of the special-token name (e.g. the vocab entry is `<|"|>`). In Lark
// string literals, `"<|\"|>"` emits those bytes directly — the model still
// produces a single vocab token because that is the canonical tokenization.

/// Recursively emit Lark rules for one JSON schema node.
/// Appends new rules to `rules` and returns the rule name to reference.
fn schema_to_lark(
    schema: &Value,
    rules: &mut Vec<String>,
    prefix: &str,
) -> Result<String, ToolFormatError> {
    debug!(json_schema = %schema);

    let type_str = schema
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("string");

    match type_str {
        "string" => {
            if let Some(enum_values) = schema.get("enum").and_then(|e| e.as_array()) {
                let rule_name = format!("{prefix}_enum");
                let alts: Vec<String> = enum_values
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| {
                        let esc = super::escape_lark_string(s);
                        format!(r#""<|\"|>" "{esc}" "<|\"|>""#)
                    })
                    .collect();
                if !alts.is_empty() {
                    rules.push(format!("{rule_name}: {}", alts.join(" | ")));
                    return Ok(rule_name);
                }
            }
            Ok("gemmafour_string".to_string())
        }
        "number" => Ok("gemmafour_number".to_string()),
        "integer" => Ok("gemmafour_integer".to_string()),
        "boolean" => Ok("gemmafour_bool".to_string()),
        "null" => Ok("gemmafour_null".to_string()),
        "object" => {
            let rule_name = format!("{prefix}_obj");
            let props = schema
                .get("properties")
                .and_then(|p| p.as_object())
                .filter(|p| !p.is_empty());

            if let Some(props) = props {
                // Known properties: emit fixed key:value sequence
                let mut parts = vec!["\"{\"".to_string()];
                for (i, (key, prop_schema)) in props.iter().enumerate() {
                    if i > 0 {
                        parts.push("\",\"".to_string());
                    }
                    let prop_rule = schema_to_lark(
                        prop_schema,
                        rules,
                        &format!("{prefix}_{i}_{}", super::sanitize_lark(key)),
                    )?;
                    parts.push(format!("\"{}\"", super::escape_lark_string(key)));
                    parts.push("\":\"".to_string());
                    parts.push(prop_rule);
                }
                parts.push("\"}\"".to_string());
                rules.push(format!("{rule_name}: {}", parts.join(" ")));
            } else {
                // Free-form object: arbitrary key:value pairs, or empty
                let default_str = serde_json::json!({"type": "string"});
                let val_schema = schema.get("additionalProperties").unwrap_or(&default_str);
                let val_rule = schema_to_lark(val_schema, rules, &format!("{prefix}_val"))?;

                let kv_rule = format!("{prefix}_kv");
                rules.push(format!("{kv_rule}: gemmafour_key \":\" {val_rule}"));

                let repeat_rule = format!("{prefix}_repeat");
                rules.push(format!("{repeat_rule}: \",\" {kv_rule}"));

                rules.push(format!(
                    "{rule_name}: \"{{\" {kv_rule} {repeat_rule}* \"}}\" | \"{{}}\""
                ));
            }
            Ok(rule_name)
        }
        "array" if schema.get("prefixItems").is_some() => {
            // Tuple: fixed positional types
            let rule_name = format!("{prefix}_arr");
            let prefix_items = schema["prefixItems"].as_array().ok_or_else(|| {
                ToolFormatError::GrammarGenerationFailed("prefixItems is not an array".into())
            })?;
            let mut parts = vec!["\"[\"".to_string()];
            for (i, item_schema) in prefix_items.iter().enumerate() {
                if i > 0 {
                    parts.push("\",\"".to_string());
                }
                let item_rule = schema_to_lark(item_schema, rules, &format!("{prefix}_{i}"))?;
                parts.push(item_rule);
            }
            parts.push("\"]\"".to_string());
            rules.push(format!("{rule_name}: {}", parts.join(" ")));
            Ok(rule_name)
        }
        "array" => {
            let rule_name = format!("{prefix}_arr");
            let default_str = serde_json::json!({"type": "string"});
            let item_schema = schema.get("items").unwrap_or(&default_str);
            let item_rule = schema_to_lark(item_schema, rules, &format!("{prefix}_item"))?;

            let repeat_rule = format!("{prefix}_repeat");
            rules.push(format!("{repeat_rule}: \",\" {item_rule}"));
            rules.push(format!(
                "{rule_name}: \"[\" {item_rule} {repeat_rule}* \"]\" | \"[]\""
            ));
            Ok(rule_name)
        }
        _ => Ok("gemmafour_string".to_string()),
    }
}

// ============================================================================
// Handler
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct Gemma4Handler;

impl ToolFormatHandler for Gemma4Handler {
    fn begin_token(&self) -> &str {
        BEGIN_TOKEN
    }

    fn end_token(&self) -> &str {
        END_TOKEN
    }

    fn to_lark(&self, tools: &[Tool]) -> Result<String, ToolFormatError> {
        let mut lark = String::from("%llguidance {}\n");
        lark.push_str("start: toolcall+\n");
        lark.push_str(&format!(
            "toolcall: \"{BEGIN_TOKEN}\" tool_alt \"{END_TOKEN}\"\n"
        ));

        let mut tool_rules: Vec<String> = Vec::new();
        let mut tool_call_names: Vec<String> = Vec::new();

        for (i, tool) in tools.iter().enumerate() {
            let prefix = format!("tool{i}");
            let params_rule = schema_to_lark(&tool.json_schema, &mut tool_rules, &prefix)?;
            let tool_rule = format!("{prefix}_call");
            tool_rules.push(format!(
                "{tool_rule}: \"call:{}\" {params_rule}",
                super::escape_lark_string(&tool.name)
            ));
            tool_call_names.push(tool_rule);
        }

        lark.push_str(&format!("tool_alt: {}\n", tool_call_names.join(" | ")));

        // Shared primitive rules referenced by schema_to_lark output.
        // The string body matches any text up to the closing `<|"|>` delimiter,
        // which the lazy `suffix` consumes. This allows values containing `<`;
        // the old `/[^<]*/` banned every `<`, not just the delimiter.
        lark.push_str(r#"gemmafour_string: "<|\"|>" gemmafour_strbody"#);
        lark.push('\n');
        lark.push_str(r#"gemmafour_strbody[suffix=/<\|"\|>/]: /(?s:.*)/"#);
        lark.push('\n');
        lark.push_str("gemmafour_key: /[a-zA-Z0-9_]+/\n");
        lark.push_str(r#"gemmafour_bool: "true" | "false""#);
        lark.push('\n');
        lark.push_str(r#"gemmafour_null: "null""#);
        lark.push('\n');
        lark.push_str("gemmafour_integer: /-?[0-9]+/\n");
        lark.push_str("gemmafour_number: /-?[0-9]+(\\.[0-9]+)?/\n");

        for rule in &tool_rules {
            lark.push_str(rule);
            lark.push('\n');
        }

        Ok(lark)
    }

    fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>> {
        // Find the first <|tool_call> and parse from there
        let start = input.find(BEGIN_TOKEN)?;
        let input = &input[start..];

        let Ok((_, calls)) = many1(single_tool_call).parse(input) else {
            debug!("No Gemma4 tool calls detected");
            return None;
        };

        Some(calls)
    }

    /// Vocabulary hint: value bytes that aren't structural delimiters (`< > { } , :`).
    fn slice_regexes(&self) -> Vec<String> {
        vec![r"[^<>{},:]+".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_begin_end_tokens() {
        let handler = Gemma4Handler;
        assert_eq!(handler.begin_token(), "<|tool_call>");
        assert_eq!(handler.end_token(), "<tool_call|>");
    }

    #[test]
    fn test_extract_mixed_types_and_multiple_params() {
        // Covers: string with <|"|>, integer, boolean, multiple params, single call
        let handler = Gemma4Handler;
        let input = r#"<|tool_call>call:search{query:<|"|>rust lang<|"|>,limit:10,exact:false}<tool_call|>"#;

        let calls = handler.extract_tool_calls(input).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search");
        assert_eq!(calls[0].arguments["query"], json!("rust lang"));
        assert_eq!(calls[0].arguments["limit"], json!(10));
        assert_eq!(calls[0].arguments["exact"], json!(false));
    }

    #[test]
    fn test_extract_multiple_calls_and_no_params() {
        // Covers: multiple sequential tool calls, empty params
        let handler = Gemma4Handler;
        let input = r#"<|tool_call>call:get_time{}<tool_call|><|tool_call>call:get_weather{city:<|"|>Paris<|"|>}<tool_call|>"#;

        let calls = handler.extract_tool_calls(input).unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "get_time");
        assert_eq!(calls[0].arguments, json!({}));
        assert_eq!(calls[1].name, "get_weather");
        assert_eq!(calls[1].arguments, json!({"city": "Paris"}));
    }

    #[test]
    fn test_extract_nested_object_and_array() {
        // Covers: nested object values, array values, float, surrounding text
        let handler = Gemma4Handler;
        let input = r#"Sure! <|tool_call>call:create_event{name:<|"|>Meeting<|"|>,location:{city:<|"|>NYC<|"|>,floor:3},tags:[<|"|>work<|"|>,<|"|>urgent<|"|>],rating:4.5}<tool_call|> Done."#;

        let calls = handler.extract_tool_calls(input).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "create_event");
        assert_eq!(calls[0].arguments["name"], json!("Meeting"));
        assert_eq!(
            calls[0].arguments["location"],
            json!({"city": "NYC", "floor": 3})
        );
        assert_eq!(calls[0].arguments["tags"], json!(["work", "urgent"]));
        assert_eq!(calls[0].arguments["rating"], json!(4.5));
    }

    #[test]
    fn test_extract_no_tool_calls() {
        let handler = Gemma4Handler;
        assert!(handler.extract_tool_calls("Just regular text.").is_none());
    }

    #[test]
    fn test_lark_smoke() {
        let handler = Gemma4Handler;
        let tools = vec![
            Tool::new(
                "get_weather",
                "Get weather",
                json!({
                    "type": "object",
                    "properties": {
                        "city": { "type": "string" }
                    },
                    "required": ["city"]
                }),
                std::sync::Arc::new(|_| "sunny".to_string()),
            ),
            Tool::new(
                "get_time",
                "Get current time",
                json!({ "type": "object", "properties": {} }),
                std::sync::Arc::new(|_| "12:00".to_string()),
            ),
        ];

        let lark = handler.to_lark(&tools).expect("lark should build");
        assert!(lark.contains("%llguidance"));
        assert!(lark.contains("<|tool_call>"));
        assert!(lark.contains("<tool_call|>"));
        assert!(lark.contains("call:get_weather"));
        assert!(lark.contains("call:get_time"));
        assert!(lark.contains("gemmafour_string"));
    }

    #[test]
    fn test_lark_scalar_types() {
        let handler = Gemma4Handler;
        let tool = Tool::new(
            "f",
            "test",
            json!({
                "type": "object",
                "properties": {
                    "n": { "type": "integer" },
                    "x": { "type": "number" },
                    "b": { "type": "boolean" },
                    "z": { "type": "null" },
                    "s": { "type": "string" }
                }
            }),
            std::sync::Arc::new(|_| String::new()),
        );
        let lark = handler.to_lark(&[tool]).expect("lark should build");
        assert!(lark.contains("gemmafour_integer"));
        assert!(lark.contains("gemmafour_number"));
        assert!(lark.contains("gemmafour_bool"));
        assert!(lark.contains("gemmafour_null"));
        assert!(lark.contains("gemmafour_string"));
    }
}
