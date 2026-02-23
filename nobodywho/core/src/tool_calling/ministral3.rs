use super::{Tool, ToolCall, ToolFormatError, ToolFormatHandler};
use gbnf::builder::{nt, GrammarBuilder};
use gbnf::json::json_schema_to_grammar;
use nom::{
    bytes::complete::{tag, take_till, take_until},
    combinator::rest,
    multi::many1,
    sequence::{preceded, separated_pair},
    IResult, Parser,
};
use tracing::debug;

#[derive(Debug, Clone, Copy)]
pub struct Ministral3Handler;

/// Parse a single tool call: [TOOL_CALLS]name[ARGS]json_arguments
fn tool_call(input: &str) -> IResult<&str, (&str, &str)> {
    preceded(
        tag("[TOOL_CALLS]"),
        separated_pair(
            take_till(|c| c == '['),
            tag("[ARGS]"),
            nom::branch::alt((take_until("[TOOL_CALLS]"), rest)),
        ),
    )
    .parse(input)
}

impl ToolFormatHandler for Ministral3Handler {
    fn begin_token(&self) -> &str {
        "[TOOL_CALLS]"
    }

    fn end_token(&self) -> &str {
        ""
    }

    fn generate_grammar(&self, tools: &[Tool]) -> Result<gbnf::GbnfGrammar, ToolFormatError> {
        // Build a per-tool grammar: "[TOOL_CALLS]" "toolname" "[ARGS]" {json-args}
        let tool_grammars: Vec<gbnf::GbnfGrammar> = tools
            .iter()
            .map(|tool| {
                let args_grammar = json_schema_to_grammar(&tool.json_schema, "root")?;
                let tool_call_grammar = gbnf::gbnf! {
                    root ::= "[TOOL_CALLS]" {&tool.name} "[ARGS]" @{args_grammar}
                };
                Ok(tool_call_grammar)
            })
            .collect::<Result<_, gbnf::json::JsonSchemaError>>()?;

        // Combine: each tool grammar is an alternative, allow one or more calls
        let mut builder = GrammarBuilder::new();
        let mut tool_refs = Vec::new();
        for (i, grammar) in tool_grammars.iter().enumerate() {
            let alias = format!("tool-{}", i);
            builder = builder.include_grammar_as(grammar, &alias);
            tool_refs.push(nt(&alias));
        }

        let grammar = builder.rule("root", alt(&tool_refs)).root("root").build();

        Ok(grammar)
    }

    fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>> {
        let Ok((_, parsed)) = many1(tool_call).parse(input) else {
            debug!("No Ministral3 tool calls detected");
            return None;
        };

        let calls: Vec<ToolCall> = parsed
            .into_iter()
            .filter_map(|(name, args_str)| {
                match serde_json::from_str(args_str.trim()) {
                    Ok(arguments) => {
                        debug!(tool_name = %name.trim(), "Parsed tool call");
                        Some(ToolCall {
                            name: name.trim().to_string(),
                            arguments,
                        })
                    }
                    Err(e) => {
                        debug!(error = %e, "Failed to parse tool call arguments");
                        None
                    }
                }
            })
            .collect();

        if calls.is_empty() {
            None
        } else {
            Some(calls)
        }
    }
}

// Tool call format looks like this:
//
// [TOOL_CALLS]sparklify[ARGS]{"text": "JULEMAND"}
//
// Jinja template:
//         {%- for tool in message['tool_calls'] %}
//             {%- set arguments = tool['function']['arguments'] %}
//             {%- if arguments is not string %}
//                 {%- set arguments = arguments|tojson|safe %}
//             {%- elif arguments == '' %}
//                 {%- set arguments = '{}' %}
//             {%- endif %}
//             {{- '[TOOL_CALLS]' + tool['function']['name'] + '[ARGS]' + arguments }}
//         {%- endfor %}
//

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_single_tool_call() {
        let handler = Ministral3Handler;
        let input = r#"[TOOL_CALLS]sparklify[ARGS]{"text": "JULEMAND"}"#;

        let result = handler.extract_tool_calls(input);
        assert!(result.is_some());

        let tool_calls = result.unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "sparklify");
        assert_eq!(tool_calls[0].arguments, json!({"text": "JULEMAND"}));
    }

    #[test]
    fn test_multiple_tool_calls() {
        let handler = Ministral3Handler;
        let input = r#"[TOOL_CALLS]tool1[ARGS]{"a": 1}[TOOL_CALLS]tool2[ARGS]{"b": 2}"#;

        let result = handler.extract_tool_calls(input);
        assert!(result.is_some());

        let tool_calls = result.unwrap();
        assert_eq!(tool_calls.len(), 2);
        assert_eq!(tool_calls[0].name, "tool1");
        assert_eq!(tool_calls[0].arguments, json!({"a": 1}));
        assert_eq!(tool_calls[1].name, "tool2");
        assert_eq!(tool_calls[1].arguments, json!({"b": 2}));
    }

    #[test]
    fn test_no_tool_calls() {
        let handler = Ministral3Handler;
        let input = "This is just regular text without any tool calls.";

        let result = handler.extract_tool_calls(input);
        assert!(result.is_none());
    }

    #[test]
    fn test_nested_json_arguments() {
        let handler = Ministral3Handler;
        let input = r#"[TOOL_CALLS]query[ARGS]{"filter": {"age": 30}, "fields": ["name", "email"]}"#;

        let result = handler.extract_tool_calls(input);
        assert!(result.is_some());

        let tool_calls = result.unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "query");
        assert_eq!(
            tool_calls[0].arguments,
            json!({"filter": {"age": 30}, "fields": ["name", "email"]})
        );
    }
}
