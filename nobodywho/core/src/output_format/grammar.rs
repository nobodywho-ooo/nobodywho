use super::{ArgsSyntax, CallParts, CallSyntax, FormatError, ResolvedFormat, ValueSyntax};
use crate::sampler::json_body_slice_regexes;
use crate::tool_calling::Tool;
use serde_json::Value;
use std::collections::HashSet;

impl ResolvedFormat {
    /// A Lark grammar that holds a response to calls of `tools` matching their
    /// schemas. It starts at `begin`, so it takes over once that is sampled.
    pub fn grammar(&self, tools: &[Tool]) -> Result<String, FormatError> {
        if tools.is_empty() {
            return Err(FormatError::NoTools);
        }
        let mut grammar = Grammar {
            format: self,
            rules: Vec::new(),
            json_like: false,
        };

        let calls = tools
            .iter()
            .enumerate()
            .map(|(i, tool)| grammar.call(i, tool))
            .collect::<Vec<_>>();

        let syntax = self.format.tool_calls();
        let body = match syntax.list {
            None => "call".to_string(),
            Some(list) => {
                let more = seq([grammar.lit(list.separator), Some("call".into())]);
                seq([
                    grammar.lit(list.open),
                    Some(format!("call ({more})*")),
                    grammar.lit(list.close),
                ])
            }
        };
        let tool_calls = seq([
            grammar.lit(syntax.begin),
            Some(body),
            syntax.end.and_then(|end| grammar.lit(end)),
        ]);

        let mut lark = String::from("%llguidance {}\n");
        lark.push_str("start: tool_calls (ws? tool_calls)* ws?\n");
        lark.push_str(&format!("tool_calls: {tool_calls}\n"));
        lark.push_str(&format!("call: {}\n", calls.join(" | ")));
        lark.push_str("ws: /[ \\t\\r\\n]+/\n");
        for rule in grammar.rules {
            lark.push_str(&rule);
            lark.push('\n');
        }
        Ok(lark)
    }

    /// Vocabulary hints for the grammar's hot positions. Only JSON strings have
    /// one that can apply, since the other string bodies are lazy.
    pub fn slice_regexes(&self) -> Vec<String> {
        match self.format.tool_calls().call {
            CallSyntax::JsonObject { .. }
            | CallSyntax::Parts(CallParts {
                args: ArgsSyntax::Json,
                ..
            })
            | CallSyntax::Parts(CallParts {
                args:
                    ArgsSyntax::KeyValue {
                        value: ValueSyntax::Json,
                        ..
                    },
                ..
            }) => json_body_slice_regexes(),
            CallSyntax::Parts(_) => vec![],
        }
    }
}

struct Grammar<'a> {
    format: &'a ResolvedFormat,
    rules: Vec<String>,
    /// Whether the rules `ValueSyntax::JsonLike` shares have been added.
    json_like: bool,
}

impl Grammar<'_> {
    /// A marker as a Lark term: by id if it's a control token, else as text.
    fn lit(&self, marker: &str) -> Option<String> {
        if marker.is_empty() {
            None
        } else if let Some(token) = self.format.control.get(marker) {
            Some(format!("<[{}]>", token.0))
        } else {
            Some(quoted(marker))
        }
    }

    fn rule(&mut self, name: &str, body: impl AsRef<str>) {
        self.rules.push(format!("{name}: {}", body.as_ref()));
    }

    fn json(&mut self, name: &str, schema: &Value) -> String {
        self.rule(
            name,
            format!("%json {}", json_schema_for_llguidance(schema)),
        );
        name.to_string()
    }

    fn call(&mut self, index: usize, tool: &Tool) -> String {
        let name = format!("call_{index}");
        let call = match self.format.format.tool_calls().call {
            CallSyntax::Parts(parts) => self.call_parts(&name, parts, tool),
            CallSyntax::JsonObject {
                name_key,
                arguments_key,
            } => {
                // In this order, as the template writes it.
                let mut properties = serde_json::Map::new();
                properties.insert(name_key.into(), serde_json::json!({ "const": tool.name }));
                properties.insert(arguments_key.into(), tool.json_schema.clone());
                let schema = serde_json::json!({
                    "type": "object",
                    "properties": properties,
                    "required": [name_key, arguments_key],
                    "additionalProperties": false,
                });
                let json = self.json(&format!("{name}_json"), &schema);
                format!("ws? {json} ws?")
            }
        };
        self.rule(&name, call);
        name
    }

    fn call_parts(&mut self, name: &str, syntax: CallParts, tool: &Tool) -> String {
        let args = match syntax.args {
            ArgsSyntax::Json => self.json(&format!("{name}_args"), &tool.json_schema),
            ArgsSyntax::KeyValue {
                before_key,
                after_key,
                after_value,
                separator,
                value,
            } => {
                let mut params = Vec::new();
                let required = required(&tool.json_schema);
                for (j, (key, schema)) in properties(&tool.json_schema).enumerate() {
                    let param = format!("{name}_p{j}_{}", sanitize_lark(key));
                    let value = self.value(&param, value, schema, after_value);
                    let kv = seq([
                        self.lit(before_key),
                        Some(quoted(key)),
                        self.lit(after_key),
                        Some(value),
                    ]);
                    self.rule(&format!("{param}_kv"), kv);
                    params.push((format!("{param}_kv"), required.contains(key.as_str())));
                }
                self.arg_list(name, &params, separator)
            }
        };
        seq([
            self.lit(syntax.before_name),
            Some(quoted(&tool.name)),
            self.lit(syntax.after_name),
            Some(args),
            self.lit(syntax.after_args),
        ])
    }

    /// Arguments in schema order, required ones always and optional ones maybe,
    /// with `separator` only between the ones present.
    fn arg_list(&mut self, prefix: &str, params: &[(String, bool)], separator: &str) -> String {
        // `from_i` has nothing written yet, `more_i` has, and both start at the
        // i-th argument.
        let n = params.len();
        let sep = self.lit(separator);
        self.rule(&format!("{prefix}_from_{n}"), "\"\"");
        self.rule(&format!("{prefix}_more_{n}"), "\"\"");
        for (i, (kv, required)) in params.iter().enumerate().rev() {
            let (from_next, more_next) = (
                format!("{prefix}_from_{}", i + 1),
                format!("{prefix}_more_{}", i + 1),
            );
            let from = format!("{kv} {more_next}");
            let more = seq([sep.clone(), Some(kv.clone()), Some(more_next.clone())]);
            if *required {
                self.rule(&format!("{prefix}_from_{i}"), from);
                self.rule(&format!("{prefix}_more_{i}"), more);
            } else {
                self.rule(
                    &format!("{prefix}_from_{i}"),
                    format!("{from} | {from_next}"),
                );
                self.rule(
                    &format!("{prefix}_more_{i}"),
                    format!("{more} | {more_next}"),
                );
            }
        }
        format!("{prefix}_from_0")
    }

    /// A value and the `after` that follows it.
    fn value(&mut self, name: &str, syntax: ValueSyntax, schema: &Value, after: &str) -> String {
        let rule = format!("{name}_val");
        let is_string = schema_type(schema) == "string";
        let variants = string_variants(schema);
        match syntax {
            ValueSyntax::Json => seq([Some(self.json(&rule, schema)), self.lit(after)]),
            ValueSyntax::Raw if is_string && variants.is_none() => {
                // The lazy body stops at the first `after`, which the suffix
                // consumes.
                self.rule(
                    &format!("{rule}[suffix={}]", lark_regex(after)),
                    "/(?s:.*)/",
                );
                rule
            }
            ValueSyntax::Raw => {
                let value = match variants {
                    Some(variants) if is_string => {
                        self.rule(&rule, variants);
                        rule
                    }
                    _ => self.json(&rule, schema),
                };
                seq([Some(value), self.lit(after)])
            }
            ValueSyntax::Delimited(quote) => {
                let value = match variants {
                    Some(variants) if is_string => {
                        let q = self.lit(quote);
                        self.rule(&rule, seq([q.clone(), Some(format!("({variants})")), q]));
                        rule
                    }
                    _ if is_string => self.quoted_text(&rule, quote),
                    _ => {
                        let json = self.json(&format!("{rule}_json"), schema);
                        let q = self.lit(quote);
                        self.rule(&rule, seq([q.clone(), Some(json), q]));
                        rule
                    }
                };
                seq([Some(value), self.lit(after)])
            }
            ValueSyntax::JsonLike { quote } => {
                seq([Some(self.json_like(&rule, schema, quote)), self.lit(after)])
            }
        }
    }

    /// Any text between two `quote`s.
    fn quoted_text(&mut self, name: &str, quote: &str) -> String {
        let q = self.lit(quote);
        if self.format.control.contains_key(quote) {
            // A control token can't occur in text, so the body can't run past it.
            self.rule(name, seq([q.clone(), Some("/(?s:.*)/".into()), q]));
        } else {
            let body = format!("{name}_body");
            self.rule(name, seq([q, Some(body.clone())]));
            self.rule(
                &format!("{body}[suffix={}]", lark_regex(quote)),
                "/(?s:.*)/",
            );
        }
        name.to_string()
    }

    fn json_like(&mut self, name: &str, schema: &Value, quote: &str) -> String {
        if !self.json_like {
            self.json_like = true;
            self.quoted_text("jl_string", quote);
            self.rule("jl_key", "/[a-zA-Z0-9_-]+/");
            self.rule("jl_bool", "\"true\" | \"false\"");
            self.rule("jl_null", "\"null\"");
            self.rule("jl_integer", "/-?[0-9]+/");
            self.rule("jl_number", "/-?[0-9]+(\\.[0-9]+)?/");
        }

        match schema_type(schema) {
            "string" => match string_variants(schema) {
                Some(variants) => {
                    let q = self.lit(quote);
                    self.rule(name, seq([q.clone(), Some(format!("({variants})")), q]));
                    name.to_string()
                }
                None => "jl_string".to_string(),
            },
            "number" => "jl_number".to_string(),
            "integer" => "jl_integer".to_string(),
            "boolean" => "jl_bool".to_string(),
            "null" => "jl_null".to_string(),
            "object" if properties(schema).next().is_some() => {
                let required = required(schema);
                let mut params = Vec::new();
                for (j, (key, prop)) in properties(schema).enumerate() {
                    let prop_name = format!("{name}_p{j}_{}", sanitize_lark(key));
                    let value = self.json_like(&format!("{prop_name}_val"), prop, quote);
                    self.rule(
                        &format!("{prop_name}_kv"),
                        format!("{} \":\" {value}", quoted(key)),
                    );
                    params.push((format!("{prop_name}_kv"), required.contains(key.as_str())));
                }
                let args = self.arg_list(name, &params, ",");
                self.rule(&format!("{name}_obj"), format!("\"{{\" {args} \"}}\""));
                format!("{name}_obj")
            }
            "object" => {
                let default = serde_json::json!({ "type": "string" });
                let value_schema = schema.get("additionalProperties").filter(|v| v.is_object());
                let value = self.json_like(
                    &format!("{name}_v"),
                    value_schema.unwrap_or(&default),
                    quote,
                );
                let kv = format!("jl_key \":\" {value}");
                self.rule(name, format!("\"{{\" ({kv} (\",\" {kv})*)? \"}}\""));
                name.to_string()
            }
            "array" => {
                if let Some(items) = schema.get("prefixItems").and_then(Value::as_array) {
                    let terms = items
                        .iter()
                        .enumerate()
                        .map(|(i, item)| self.json_like(&format!("{name}_{i}"), item, quote))
                        .collect::<Vec<_>>();
                    self.rule(name, format!("\"[\" {} \"]\"", terms.join(" \",\" ")));
                } else {
                    let default = serde_json::json!({ "type": "string" });
                    let items = schema.get("items").unwrap_or(&default);
                    let item = self.json_like(&format!("{name}_item"), items, quote);
                    self.rule(name, format!("\"[\" ({item} (\",\" {item})*)? \"]\""));
                }
                name.to_string()
            }
            _ => "jl_string".to_string(),
        }
    }
}

/// Terms joined into a sequence, skipping empty ones. An empty sequence is the
/// empty string.
fn seq(terms: impl IntoIterator<Item = Option<String>>) -> String {
    let terms: Vec<String> = terms.into_iter().flatten().collect();
    if terms.is_empty() {
        "\"\"".to_string()
    } else {
        terms.join(" ")
    }
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", escape_lark_string(text))
}

/// Escapes text for a double-quoted Lark literal, where a raw newline, tab or
/// carriage return would also break it.
pub(super) fn escape_lark_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Lowercase alphanumerics and `_`, for rule names made from tool and property
/// names. Lark reads an uppercase name as a terminal.
pub(super) fn sanitize_lark(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// A schema for a `%json` directive, tagged `lenient` so llguidance ignores
/// keywords it doesn't implement instead of failing, while still enforcing the
/// ones it does. The tag only works at the root of each `%json`.
pub(super) fn json_schema_for_llguidance(schema: &Value) -> String {
    let mut schema = schema.clone();
    if let Some(object) = schema.as_object_mut() {
        object.insert(
            "x-guidance".to_string(),
            serde_json::json!({ "lenient": true }),
        );
    }
    schema.to_string()
}

/// `text` as a Lark regex matching exactly it.
fn lark_regex(text: &str) -> String {
    let escaped = regex::escape(text)
        .replace('/', "\\/")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("/{escaped}/")
}

/// A schema's type, taken to be string when it doesn't say.
pub(super) fn schema_type(schema: &Value) -> &str {
    schema
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("string")
}

/// A string enum's variants as Lark alternatives.
fn string_variants(schema: &Value) -> Option<String> {
    let variants: Vec<String> = schema
        .get("enum")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(quoted)
        .collect();
    (!variants.is_empty()).then(|| variants.join(" | "))
}

pub(super) fn properties(schema: &Value) -> impl Iterator<Item = (&String, &Value)> {
    schema
        .get("properties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
}

fn required(schema: &Value) -> HashSet<&str> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}
