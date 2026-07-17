use super::{Tool, ToolCall, ToolFormatError, ToolFormatHandler};
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::sync::OnceLock;
use tracing::debug;

const BEGIN_TOKEN: &str = "<tool_call>";
const END_TOKEN: &str = "</tool_call>";

#[derive(Debug, Clone, Copy)]
pub struct Qwen35_36Handler;

impl ToolFormatHandler for Qwen35_36Handler {
    fn begin_token(&self) -> &str {
        BEGIN_TOKEN
    }

    fn end_token(&self) -> &str {
        END_TOKEN
    }

    fn to_lark(&self, tools: &[Tool]) -> Result<String, ToolFormatError> {
        let mut lark = String::from("%llguidance {}\n");
        lark.push_str("start: toolcall+\n");

        let alts: Vec<String> = (0..tools.len()).map(|i| format!("tool_{i}")).collect();
        lark.push_str("toolcall: \"<tool_call>\\n\" tool_alt \"</tool_call>\"\n");
        lark.push_str(&format!("tool_alt: {}\n", alts.join(" | ")));

        for (ti, tool) in tools.iter().enumerate() {
            let tprefix = format!("qwen35_t{ti}");
            let required = required_params(tool);

            let mut block_rules: Vec<(String, bool)> = Vec::new();

            if let Some(props) = tool
                .json_schema
                .get("properties")
                .and_then(|p| p.as_object())
            {
                for (pi, (pname, pschema)) in props.iter().enumerate() {
                    let pprefix = format!("{tprefix}_p{pi}_{}", super::sanitize_lark(pname));
                    let block_rule = format!("{pprefix}_block");
                    let is_required = required.contains(pname.as_str());
                    let pname = super::escape_lark_string(pname);

                    let ty = pschema
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("string");

                    if ty == "string" {
                        if let Some(variants) = pschema.get("enum").and_then(|e| e.as_array()) {
                            let enum_alts: Vec<String> = variants
                                .iter()
                                .filter_map(|v| v.as_str())
                                .map(|s| format!("\"{}\"", super::escape_lark_string(s)))
                                .collect();
                            if !enum_alts.is_empty() {
                                lark.push_str(&format!(
                                    "{block_rule}: \"<parameter={pname}>\\n\" ({}) \"\\n</parameter>\\n\"\n",
                                    enum_alts.join(" | ")
                                ));
                                block_rules.push((block_rule, is_required));
                                continue;
                            }
                        }
                        // Free-form string: must not contain \n< (the start of the </parameter> terminator)
                        lark.push_str(&format!(
                            "{block_rule}: \"<parameter={pname}>\\n\" /([^\\n]|\\n[^<])*/ \"\\n</parameter>\\n\"\n"
                        ));
                    } else {
                        let val_rule = format!("{pprefix}_val");
                        let schema_str = serde_json::to_string(pschema)
                            .map_err(|e| ToolFormatError::GrammarGenerationFailed(e.to_string()))?;
                        lark.push_str(&format!("{val_rule}: %json {schema_str}\n"));
                        lark.push_str(&format!(
                            "{block_rule}: \"<parameter={pname}>\\n\" {val_rule} \"\\n</parameter>\\n\"\n"
                        ));
                    }

                    block_rules.push((block_rule, is_required));
                }
            }

            let mut rule = format!(
                "tool_{ti}: \"<function={}>\\n\"",
                super::escape_lark_string(&tool.name)
            );
            for (block_rule, is_required) in &block_rules {
                if *is_required {
                    rule.push_str(&format!(" {block_rule}"));
                } else {
                    rule.push_str(&format!(" {block_rule}?"));
                }
            }
            rule.push_str(" \"</function>\\n\"");
            lark.push_str(&rule);
            lark.push('\n');
        }

        Ok(lark)
    }

    fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>> {
        let calls: Vec<ToolCall> = outer_tool_call_regex()
            .captures_iter(input)
            .filter_map(parse_tool_call_capture)
            .collect();

        (!calls.is_empty()).then_some(calls)
    }
}

fn required_params(tool: &Tool) -> HashSet<&str> {
    tool.json_schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default()
}

// ============================================================================
// Parsing
// ============================================================================

fn outer_tool_call_regex() -> &'static Regex {
    static OUTER: OnceLock<Regex> = OnceLock::new();
    OUTER.get_or_init(|| {
        Regex::new(r"(?s)<tool_call>\s*<function=([^>\s]+)>\s*(.*?)\s*</function>\s*</tool_call>")
            .expect("outer Qwen3.5/3.6 tool regex should compile")
    })
}

fn parameter_regex() -> &'static Regex {
    static PARAM: OnceLock<Regex> = OnceLock::new();
    PARAM.get_or_init(|| {
        Regex::new(r"(?s)<parameter=([^>\s]+)>\n(.*?)\n</parameter>")
            .expect("parameter Qwen3.5/3.6 regex should compile")
    })
}

fn parse_parameter_arguments(body: &str) -> Map<String, Value> {
    parameter_regex()
        .captures_iter(body)
        .filter_map(|p| {
            let key = p.get(1)?.as_str().to_string();
            let raw = p.get(2)?.as_str();
            let value = serde_json::from_str::<Value>(raw)
                .unwrap_or_else(|_| Value::String(raw.to_string()));
            Some((key, value))
        })
        .collect()
}

fn parse_tool_call_capture(cap: regex::Captures<'_>) -> Option<ToolCall> {
    let name = cap.get(1)?.as_str().to_string();
    let body = cap.get(2)?.as_str();
    let arguments = parse_parameter_arguments(body);
    debug!(tool_name = %name, ?arguments, "parsed Qwen3.5 tool call");

    Some(ToolCall {
        name,
        arguments: Value::Object(arguments),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tokens() {
        let h = Qwen35_36Handler;
        assert_eq!(h.begin_token(), "<tool_call>");
        assert_eq!(h.end_token(), "</tool_call>");
    }

    #[test]
    fn extract_single_string_param() {
        let h = Qwen35_36Handler;
        let input = "<tool_call>\n<function=get_weather>\n<parameter=city>\nCopenhagen\n</parameter>\n</function>\n</tool_call>";
        let calls = h.extract_tool_calls(input).expect("should parse");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].arguments, json!({"city": "Copenhagen"}));
    }

    #[test]
    fn extract_scalar_params() {
        let h = Qwen35_36Handler;
        let input = "<tool_call>\n<function=f>\n<parameter=n>\n42\n</parameter>\n<parameter=b>\ntrue\n</parameter>\n</function>\n</tool_call>";
        let calls = h.extract_tool_calls(input).expect("should parse");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].arguments, json!({"n": 42, "b": true}));
    }

    #[test]
    fn extract_object_param() {
        let h = Qwen35_36Handler;
        let input = "<tool_call>\n<function=f>\n<parameter=opts>\n{\"a\": 1, \"b\": [2, 3]}\n</parameter>\n</function>\n</tool_call>";
        let calls = h.extract_tool_calls(input).expect("should parse");
        assert_eq!(calls[0].arguments, json!({"opts": {"a": 1, "b": [2, 3]}}));
    }

    #[test]
    fn extract_multiple_calls() {
        let h = Qwen35_36Handler;
        let input = "<tool_call>\n<function=one>\n<parameter=x>\n1\n</parameter>\n</function>\n</tool_call><tool_call>\n<function=two>\n<parameter=y>\nhi\n</parameter>\n</function>\n</tool_call>";
        let calls = h.extract_tool_calls(input).expect("should parse");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "one");
        assert_eq!(calls[1].name, "two");
        assert_eq!(calls[1].arguments, json!({"y": "hi"}));
    }

    #[test]
    fn extract_no_calls() {
        let h = Qwen35_36Handler;
        assert!(h.extract_tool_calls("plain text").is_none());
    }

    #[test]
    fn lark_builds_for_typical_schema() {
        let h = Qwen35_36Handler;
        let tool = Tool {
            name: "get_weather".to_string(),
            description: "Get weather".to_string(),
            json_schema: json!({
                "type": "object",
                "properties": {
                    "city": {"type": "string"},
                    "units": {"type": "string", "enum": ["celsius", "fahrenheit"]},
                    "verbose": {"type": "boolean"}
                },
                "required": ["city"]
            }),
            function: std::sync::Arc::new(|_| "".to_string()),
        };
        let lark = h.to_lark(&[tool]).expect("lark should build");
        assert!(lark.contains("%llguidance"));
        assert!(lark.contains("<tool_call>"));
        assert!(lark.contains("<function=get_weather>"));
        assert!(lark.contains("<parameter=city>"));
        assert!(lark.contains("<parameter=units>"));
        assert!(lark.contains("<parameter=verbose>"));
    }

    #[test]
    fn lark_includes_scalar_types() {
        let h = Qwen35_36Handler;
        let tool = Tool {
            name: "f".to_string(),
            description: "test".to_string(),
            json_schema: json!({
                "type": "object",
                "properties": {
                    "n": {"type": "integer"},
                    "x": {"type": "number"},
                    "b": {"type": "boolean"},
                    "z": {"type": "null"}
                }
            }),
            function: std::sync::Arc::new(|_| "".to_string()),
        };
        let lark = h.to_lark(&[tool]).expect("lark should build");
        assert!(lark.contains("%json"));
        assert!(lark.contains("<parameter=n>"));
        assert!(lark.contains("<parameter=x>"));
        assert!(lark.contains("<parameter=b>"));
        assert!(lark.contains("<parameter=z>"));
    }
}
