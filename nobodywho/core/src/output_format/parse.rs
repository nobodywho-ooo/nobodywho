use super::grammar::schema_type;
use super::{ArgsSyntax, CallParts, CallSyntax, ResolvedFormat, ValueSyntax};
use crate::tool_calling::{Tool, ToolCall};
use serde_json::{Map, Value};

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("expected {expected} at byte {at}")]
pub struct ParseError {
    pub at: usize,
    pub expected: String,
}

impl ResolvedFormat {
    /// The calls in one block of tool calls, given the text between its
    /// `begin` and `end`. `tools` is only used to read argument values by
    /// their schema type.
    pub fn parse_tool_calls(
        &self,
        text: &str,
        tools: &[Tool],
    ) -> Result<Vec<ToolCall>, ParseError> {
        let mut p = Parser {
            input: text,
            rest: text,
            tools,
        };
        let syntax = self.format.tool_calls();
        let calls = match syntax.list {
            None => vec![p.call(&syntax.call)?],
            Some(list) => {
                p.eat(list.open)?;
                let mut calls = vec![p.call(&syntax.call)?];
                while p.try_eat(list.separator) {
                    calls.push(p.call(&syntax.call)?);
                }
                p.eat(list.close)?;
                calls
            }
        };
        p.rest = p.rest.trim_start();
        if !p.rest.is_empty() {
            return Err(p.error("the end of the block"));
        }
        Ok(calls)
    }
}

struct Parser<'a> {
    input: &'a str,
    rest: &'a str,
    tools: &'a [Tool],
}

impl<'a> Parser<'a> {
    fn error(&self, expected: impl Into<String>) -> ParseError {
        ParseError {
            at: self.input.len() - self.rest.len(),
            expected: expected.into(),
        }
    }

    /// Consumes `marker` if `rest` starts with it. An empty marker never
    /// matches, so a loop on it can't spin.
    fn try_eat(&mut self, marker: &str) -> bool {
        match match_len(self.rest, marker) {
            Some(len) if !marker.is_empty() => {
                self.rest = &self.rest[len..];
                true
            }
            _ => false,
        }
    }

    fn eat(&mut self, marker: &str) -> Result<(), ParseError> {
        if marker.is_empty() || self.try_eat(marker) {
            Ok(())
        } else {
            Err(self.error(format!("{marker:?}")))
        }
    }

    /// Consumes the text before the next `marker`, leaving the marker.
    fn until(&mut self, marker: &str) -> Result<&'a str, ParseError> {
        let exact = self.rest.find(marker);
        let at = exact.or_else(|| {
            marker.contains(char::is_whitespace).then(|| {
                self.rest
                    .char_indices()
                    .map(|(i, _)| i)
                    .find(|&i| match_len(&self.rest[i..], marker).is_some())
            })?
        });
        match at {
            Some(at) if !marker.is_empty() => {
                let (before, rest) = self.rest.split_at(at);
                self.rest = rest;
                Ok(before)
            }
            _ => Err(self.error(format!("{marker:?}"))),
        }
    }

    fn call(&mut self, syntax: &CallSyntax) -> Result<ToolCall, ParseError> {
        match *syntax {
            CallSyntax::Parts(parts) => self.call_parts(&parts),
            CallSyntax::JsonObject {
                name_key,
                arguments_key,
            } => {
                let at = self.error(format!(
                    "a JSON object with {name_key:?} and {arguments_key:?}"
                ));
                let Value::Object(mut call) = self.json()? else {
                    return Err(at);
                };
                match (call.remove(name_key), call.remove(arguments_key)) {
                    (Some(Value::String(name)), Some(arguments @ Value::Object(_))) => {
                        Ok(ToolCall { name, arguments })
                    }
                    _ => Err(at),
                }
            }
        }
    }

    fn call_parts(&mut self, syntax: &CallParts) -> Result<ToolCall, ParseError> {
        self.eat(syntax.before_name)?;
        let name = self.until(syntax.after_name)?.trim();
        if name.is_empty() {
            return Err(self.error("a tool name"));
        }
        self.eat(syntax.after_name)?;

        let schema = self
            .tools
            .iter()
            .find(|tool| tool.name == name)
            .map(|tool| &tool.json_schema);
        let arguments = match syntax.args {
            ArgsSyntax::Json => {
                let at = self.error("a JSON object");
                match self.json()? {
                    arguments @ Value::Object(_) => arguments,
                    _ => return Err(at),
                }
            }
            ArgsSyntax::KeyValue {
                before_key,
                after_key,
                after_value,
                separator,
                value,
            } => {
                let mut arguments = Map::new();
                while match_len(self.rest, syntax.after_args).is_none() {
                    if !arguments.is_empty() {
                        self.eat(separator)?;
                    }
                    self.eat(before_key)?;
                    let key = self.until(after_key)?.trim().to_string();
                    self.eat(after_key)?;
                    let schema = schema.and_then(|s| s.get("properties")?.get(&key));
                    let value = self.value(value, schema, after_value)?;
                    arguments.insert(key, value);
                }
                Value::Object(arguments)
            }
        };
        self.eat(syntax.after_args)?;
        Ok(ToolCall {
            name: name.to_string(),
            arguments,
        })
    }

    /// A value and the `after` that ends it.
    fn value(
        &mut self,
        syntax: ValueSyntax,
        schema: Option<&Value>,
        after: &str,
    ) -> Result<Value, ParseError> {
        let value = match syntax {
            ValueSyntax::Json => self.lenient_json()?,
            ValueSyntax::Raw => typed(self.until(after)?, schema),
            ValueSyntax::Delimited(quote) => {
                self.eat(quote)?;
                let text = self.until(quote)?;
                self.eat(quote)?;
                typed(text, schema)
            }
            ValueSyntax::JsonLike { quote } => self.json_like(quote)?,
        };
        self.eat(after)?;
        Ok(value)
    }

    /// One JSON value, exactly.
    fn json(&mut self) -> Result<Value, ParseError> {
        let mut values = serde_json::Deserializer::from_str(self.rest).into_iter::<Value>();
        match values.next() {
            Some(Ok(value)) => {
                self.rest = &self.rest[values.byte_offset()..];
                Ok(value)
            }
            _ => Err(self.error("JSON")),
        }
    }

    /// One JSON value, or one of the Python literals models sometimes write
    /// instead. Anything else up to the next delimiter is read as a string.
    fn lenient_json(&mut self) -> Result<Value, ParseError> {
        self.rest = self.rest.trim_start();
        let len = value_len(self.rest).ok_or_else(|| self.error("a value"))?;
        let (text, rest) = self.rest.split_at(len);
        self.rest = rest;
        Ok(serde_json::from_str(text).unwrap_or_else(|_| match text {
            "True" => Value::Bool(true),
            "False" => Value::Bool(false),
            "None" => Value::Null,
            _ => match text.strip_prefix('\'').and_then(|t| t.strip_suffix('\'')) {
                Some(quoted) => Value::String(quoted.replace("\\'", "'")),
                None => Value::String(text.to_string()),
            },
        }))
    }

    fn json_like(&mut self, quote: &str) -> Result<Value, ParseError> {
        self.rest = self.rest.trim_start();
        if self.try_eat(quote) {
            let text = self.until(quote)?;
            self.eat(quote)?;
            return Ok(Value::String(text.to_string()));
        }
        if self.try_eat("{") {
            let mut object = Map::new();
            if !self.try_eat("}") {
                loop {
                    self.rest = self.rest.trim_start();
                    let len = self
                        .rest
                        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
                        .unwrap_or(self.rest.len());
                    if len == 0 {
                        return Err(self.error("a key"));
                    }
                    let (key, rest) = self.rest.split_at(len);
                    self.rest = rest;
                    self.eat(":")?;
                    object.insert(key.to_string(), self.json_like(quote)?);
                    if self.try_eat("}") {
                        break;
                    }
                    self.eat(",")?;
                }
            }
            return Ok(Value::Object(object));
        }
        if self.try_eat("[") {
            let mut array = Vec::new();
            if !self.try_eat("]") {
                loop {
                    array.push(self.json_like(quote)?);
                    if self.try_eat("]") {
                        break;
                    }
                    self.eat(",")?;
                }
            }
            return Ok(Value::Array(array));
        }
        let len = self
            .rest
            .find(|c: char| matches!(c, ',' | '}' | ']') || c.is_whitespace())
            .unwrap_or(self.rest.len());
        let (text, rest) = self.rest.split_at(len);
        match serde_json::from_str::<Value>(text) {
            Ok(value @ (Value::Bool(_) | Value::Null | Value::Number(_))) => {
                self.rest = rest;
                Ok(value)
            }
            _ => Err(self.error("a value")),
        }
    }
}

/// How much of `text` matches `marker`, where whitespace in the marker matches
/// any amount of whitespace, including none.
fn match_len(text: &str, marker: &str) -> Option<usize> {
    let mut rest = text;
    let mut chars = marker.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            while chars.next_if(|c| c.is_whitespace()).is_some() {}
            rest = rest.trim_start();
        } else {
            rest = rest.strip_prefix(c)?;
        }
    }
    Some(text.len() - rest.len())
}

/// The length of the value `text` starts with: a quoted string, a bracketed
/// value, or a bare word up to a delimiter.
fn value_len(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (i, c) in text.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '[' | '{' | '(' => depth += 1,
            ']' | '}' | ')' if depth == 0 => return (i > 0).then_some(i),
            ']' | '}' | ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            ',' if depth == 0 => return (i > 0).then_some(i),
            c if c.is_whitespace() && depth == 0 => return (i > 0).then_some(i),
            _ => {}
        }
    }
    (depth == 0 && quote.is_none() && !text.is_empty()).then_some(text.len())
}

/// Text read as its schema type says: as it is for a string, else as JSON.
/// Without a schema, whatever parses as JSON is JSON.
fn typed(text: &str, schema: Option<&Value>) -> Value {
    match schema {
        Some(schema) if schema_type(schema) == "string" => Value::String(text.to_string()),
        _ => serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string())),
    }
}
