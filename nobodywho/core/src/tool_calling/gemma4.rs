use super::{Tool, ToolCall, ToolFormatError, ToolFormatHandler};
use gbnf::builder::{alt, nt, nt_plus, seq, t, GrammarBuilder};
use gbnf::{Expr, GbnfGrammar, Quantifier, TokenRef};
use gbnf_macro::gbnf;
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
// Grammar generation helpers
// ============================================================================

/// Create the Expr for the <|"|> special token
fn quote_token_expr() -> Expr {
    Expr::Token(TokenRef::ByString {
        name: r#"|"|"#.to_string(),
        negated: false,
    })
}

/// Create the Expr for the <|tool_call> special token
fn begin_token_expr() -> Expr {
    Expr::Token(TokenRef::ByString {
        name: "|tool_call".to_string(),
        negated: false,
    })
}

/// Create the Expr for the <tool_call|> special token
fn end_token_expr() -> Expr {
    Expr::Token(TokenRef::ByString {
        name: "tool_call|".to_string(),
        negated: false,
    })
}

/// Walk a JSON schema and add grammar rules to the builder.
/// Returns the rule name to reference and the updated builder.
fn schema_to_rule(
    schema: &Value,
    mut builder: GrammarBuilder<gbnf::builder::NoRoot>,
    prefix: &str,
) -> Result<(String, GrammarBuilder<gbnf::builder::NoRoot>), ToolFormatError> {
    debug!(json_schema = %schema);

    let type_str = schema
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("string");

    match type_str {
        "string" => {
            if let Some(enum_values) = schema.get("enum").and_then(|e| e.as_array()) {
                // enum: alternation of literal string values
                let rule_name = format!("{}-enum", prefix);
                let alts: Vec<Expr> = enum_values
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| seq(&[quote_token_expr(), t(s), quote_token_expr()]))
                    .collect();
                builder = builder.rule(&rule_name, alt(&alts));
                Ok((rule_name, builder))
            } else {
                Ok(("gemmafour-string".to_string(), builder))
            }
        }
        "number" => Ok(("gemmafour-number".to_string(), builder)),
        "integer" => Ok(("gemmafour-integer".to_string(), builder)),
        "boolean" => Ok(("gemmafour-boolean".to_string(), builder)),
        "null" => Ok(("gemmafour-null".to_string(), builder)),
        "object" => {
            let rule_name = format!("{}-obj", prefix);
            let props = schema
                .get("properties")
                .and_then(|p| p.as_object())
                .filter(|p| !p.is_empty());

            if let Some(props) = props {
                // Known properties: fixed key:value pairs
                let mut items: Vec<Expr> = vec![t("{")];
                for (i, (key, prop_schema)) in props.iter().enumerate() {
                    if i > 0 {
                        items.push(t(","));
                    }
                    let prop_prefix = format!("{}-{}", prefix, key.replace('_', "-"));
                    let (prop_rule, new_builder) =
                        schema_to_rule(prop_schema, builder, &prop_prefix)?;
                    builder = new_builder;
                    items.push(t(key));
                    items.push(t(":"));
                    items.push(nt(&prop_rule));
                }
                items.push(t("}"));
                builder = builder.rule(&rule_name, seq(&items));
            } else {
                // Free-form object: arbitrary key:value pairs
                let default_val = Value::Object(serde_json::Map::from_iter([(
                    "type".to_string(),
                    Value::String("string".to_string()),
                )]));
                let val_schema = schema.get("additionalProperties").unwrap_or(&default_val);
                let val_prefix = format!("{}-val", prefix);
                let (val_rule, new_builder) = schema_to_rule(val_schema, builder, &val_prefix)?;
                builder = new_builder;

                let kv_rule = format!("{}-kv", prefix);
                builder =
                    builder.rule(&kv_rule, seq(&[nt("gemmafour-key"), t(":"), nt(&val_rule)]));
                let repeat_rule = format!("{}-repeat", prefix);
                builder = builder.rule(&repeat_rule, seq(&[t(","), nt(&kv_rule)]));
                builder = builder.rule(
                    &rule_name,
                    alt(&[
                        seq(&[
                            t("{"),
                            nt(&kv_rule),
                            Expr::Quantified {
                                expr: Box::new(nt(&repeat_rule)),
                                quantifier: Quantifier::ZeroOrMore,
                            },
                            t("}"),
                        ]),
                        seq(&[t("{"), t("}")]),
                    ]),
                );
            }
            Ok((rule_name, builder))
        }
        "array" if schema.get("prefixItems").is_some() => {
            // Tuple: fixed positional types, e.g. [string, integer]
            let rule_name = format!("{}-arr", prefix);
            let prefix_items = schema["prefixItems"].as_array().ok_or_else(|| {
                ToolFormatError::GrammarGenerationFailed("prefixItems is not an array".into())
            })?;
            let mut elems: Vec<Expr> = vec![t("[")];
            for (i, item_schema) in prefix_items.iter().enumerate() {
                if i > 0 {
                    elems.push(t(","));
                }
                let (item_rule, b) =
                    schema_to_rule(item_schema, builder, &format!("{}-{}", prefix, i))?;
                builder = b;
                elems.push(nt(&item_rule));
            }
            elems.push(t("]"));
            builder = builder.rule(&rule_name, seq(&elems));
            Ok((rule_name, builder))
        }
        "array" => {
            let rule_name = format!("{}-arr", prefix);
            let default_items = Value::Object(serde_json::Map::from_iter([(
                "type".to_string(),
                Value::String("string".to_string()),
            )]));
            let item_schema = schema.get("items").unwrap_or(&default_items);
            let (item_rule, b) = schema_to_rule(item_schema, builder, &format!("{}-item", prefix))?;
            builder = b;

            let repeat_rule = format!("{}-repeat", prefix);
            builder = builder.rule(&repeat_rule, seq(&[t(","), nt(&item_rule)]));
            builder = builder.rule(
                &rule_name,
                alt(&[
                    seq(&[
                        t("["),
                        nt(&item_rule),
                        Expr::Quantified {
                            expr: Box::new(nt(&repeat_rule)),
                            quantifier: Quantifier::ZeroOrMore,
                        },
                        t("]"),
                    ]),
                    seq(&[t("["), t("]")]),
                ]),
            );
            Ok((rule_name, builder))
        }
        _ => Ok(("gemmafour-string".to_string(), builder)),
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

    fn generate_grammar(&self, tools: &[Tool]) -> Result<GbnfGrammar, ToolFormatError> {
        // Step 1: Static primitives via gbnf! macro
        let primitives = gbnf! {
            gemmafour-boolean ::= "true" | "false"
            gemmafour-null ::= "null"
            gemmafour-digit ::= [0-9]
            gemmafour-integer ::= "-"? gemmafour-digit+
            gemmafour-number ::= gemmafour-integer ("." gemmafour-digit+)?
            root ::= gemmafour-number
        };

        // Step 2: Extend with gemmafour-string via builder.
        // Strings are delimited by <|"|>, so a string char is anything that isn't that token.
        let mut builder = GrammarBuilder::from_existing(primitives)
            .rule(
                "gemmafour-strchar",
                Expr::Token(TokenRef::ByString {
                    name: r#"|"|"#.to_string(),
                    negated: true,
                }),
            )
            .rule(
                "gemmafour-string",
                seq(&[
                    quote_token_expr(),
                    Expr::Quantified {
                        expr: Box::new(nt("gemmafour-strchar")),
                        quantifier: Quantifier::ZeroOrMore,
                    },
                    quote_token_expr(),
                ]),
            )
            .rule(
                "gemmafour-keychar",
                alt(&[
                    Expr::CharacterRange(gbnf::CharacterRange::Range {
                        begin: 'a',
                        end: 'z',
                        negated: false,
                    }),
                    Expr::CharacterRange(gbnf::CharacterRange::Range {
                        begin: 'A',
                        end: 'Z',
                        negated: false,
                    }),
                    Expr::CharacterRange(gbnf::CharacterRange::Range {
                        begin: '0',
                        end: '9',
                        negated: false,
                    }),
                    t("_"),
                ]),
            )
            .rule(
                "gemmafour-key",
                Expr::Quantified {
                    expr: Box::new(nt("gemmafour-keychar")),
                    quantifier: Quantifier::OneOrMore,
                },
            );

        // Step 3: Per-tool rules from JSON schema
        let mut tool_rule_names = Vec::new();
        for (i, tool) in tools.iter().enumerate() {
            let prefix = format!("tool{}", i);
            let (params_rule, new_builder) = schema_to_rule(&tool.json_schema, builder, &prefix)?;
            builder = new_builder;

            let tool_rule = format!("tool{}-call", i);
            builder = builder.rule(
                &tool_rule,
                seq(&[t("call:"), t(&tool.name), nt(&params_rule)]),
            );
            tool_rule_names.push(tool_rule);
        }

        // Step 4: Combine tools
        let tool_alts: Vec<Expr> = tool_rule_names.iter().map(|n| nt(n)).collect();
        let grammar = builder
            .rule("tool-alt", alt(&tool_alts))
            .rule(
                "toolcall",
                seq(&[begin_token_expr(), nt("tool-alt"), end_token_expr()]),
            )
            .rule("superroot", nt_plus("toolcall"))
            .root("superroot")
            .build();

        Ok(grammar)
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
    fn test_generate_grammar_smoke() {
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

        let result = handler.generate_grammar(&tools);
        assert!(
            result.is_ok(),
            "Grammar generation failed: {:?}",
            result.err()
        );
    }
}
