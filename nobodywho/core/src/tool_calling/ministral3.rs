use gbnf::builder::{nt, nt_plus, seq, t, GrammarBuilder};
use super::{Tool, ToolCall, ToolFormatError, ToolFormatHandler};
use gbnf::json::json_schema_to_grammar;
use serde_json::json;
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
        let tool_call_schemas: Vec<gbnf::GbnfGrammar> = tools
            .iter()
            .map(|tool| {
                let args_json_schema = tool.json_schema;
                let args_nonterminal = format!("{}-args", tool.name);
                // sparklify-args ::= <args-grammar-from-json-schema>
                let json_schema_grammar =
                    json_schema_to_grammar(args_json_schema, &args_nonterminal);
                // sparklify ::= "[TOOL_CALLS]" "sparklify" "[ARGS]" sparklify-args
                GrammarBuilder::new()
                    .rule(
                        &tool.name,
                        seq(&[t(self.begin_token()), t(&tool.name), t("[ARGS]")]),
                    )
                    .build()
            })
            .collect();

        // let tool_call_schema = json!(
        //     { "oneOf": tool_call_schemas }
        // );

        todo!()
        // Generate JSON grammar from schema, then extend it with wrapping rules
        // let json_grammar = json_schema_to_grammar(tool_call_schema)?;

        // let grammar = GrammarBuilder::from_existing(json_grammar)
        //     .rule(
        //         "toolcall",
        //         seq(&[t(self.begin_token()), nt("root"), t(self.end_token())]),
        //     )
        //     .rule("superroot", nt_plus("toolcall"))
        //     .build();
        // Ok(grammar)
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
