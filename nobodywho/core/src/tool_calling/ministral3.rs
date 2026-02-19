use super::{Tool, ToolCall, ToolFormatError, ToolFormatHandler};
use gbnf::builder::{alt, nt, nt_plus, GrammarBuilder};
use gbnf::json::json_schema_to_grammar;
use tracing::debug;

#[derive(Debug, Clone, Copy)]
pub struct Ministral3Handler;

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

        builder = builder
            .rule("toolcall", alt(&tool_refs))
            .rule("root", nt_plus("toolcall"));

        let grammar = builder.build();

        Ok(grammar)
    }

    fn extract_tool_calls(&self, input: &str) -> Option<Vec<ToolCall>> {
        // Split on [TOOL_CALLS] and parse each segment as JSON
        let tool_calls: Vec<ToolCall> = input
            .split("[TOOL_CALLS]")
            .filter_map(|segment| {
                let json_str = segment.trim();
                if json_str.is_empty() {
                    return None;
                }
                match serde_json::from_str::<ToolCall>(json_str) {
                    Ok(tool_call) => {
                        debug!(tool_name = %tool_call.name, "Successfully parsed tool call");
                        Some(tool_call)
                    }
                    Err(e) => {
                        debug!(error = %e, json = json_str, "Failed to parse tool call JSON");
                        None
                    }
                }
            })
            .collect();

        if !tool_calls.is_empty() {
            Some(tool_calls)
        } else {
            debug!("No tool calls detected in message");
            None
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
