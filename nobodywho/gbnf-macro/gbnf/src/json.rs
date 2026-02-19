//! JSON Schema to GBNF Grammar conversion
//!
//! This module provides functionality to convert JSON Schema definitions
//! into GBNF (GGML BNF) grammars that can be used for constrained generation.

use crate::{Expr, GbnfDeclaration, GbnfGrammar, Quantifier, gbnf};
use serde_json::Value;
use std::collections::HashMap;

/// Error type for JSON Schema conversion
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonSchemaError {
    /// The schema is not a valid JSON schema
    InvalidSchema(String),
    /// Unsupported JSON Schema feature
    UnsupportedFeature(String),
    /// Reference could not be resolved
    UnresolvedRef(String),
    /// Invalid JSON input
    InvalidJson(String),
}

impl std::fmt::Display for JsonSchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonSchemaError::InvalidSchema(msg) => write!(f, "Invalid schema: {}", msg),
            JsonSchemaError::UnsupportedFeature(msg) => write!(f, "Unsupported feature: {}", msg),
            JsonSchemaError::UnresolvedRef(msg) => write!(f, "Unresolved reference: {}", msg),
            JsonSchemaError::InvalidJson(msg) => write!(f, "Invalid JSON: {}", msg),
        }
    }
}

impl std::error::Error for JsonSchemaError {}

/// Converter from JSON Schema to GBNF Grammar
pub struct JsonSchemaConverter {
    /// Generated declarations
    declarations: Vec<GbnfDeclaration>,
    /// Counter for generating unique rule names
    rule_counter: usize,
    /// Cache of definitions for $ref resolution
    definitions: HashMap<String, Value>,
    /// Track which definitions have been converted to avoid duplicates
    converted_refs: HashMap<String, String>,
}

impl JsonSchemaConverter {
    /// Create a new converter
    pub fn new() -> Self {
        Self {
            declarations: Vec::new(),
            rule_counter: 0,
            definitions: HashMap::new(),
            converted_refs: HashMap::new(),
        }
    }

    /// Convert a JSON Schema value to a GBNF Grammar
    pub fn convert(&mut self, schema: &Value, root: &str) -> Result<GbnfGrammar, JsonSchemaError> {
        // Reset state
        self.declarations.clear();
        self.rule_counter = 0;
        self.converted_refs.clear();

        // Extract definitions if present
        self.extract_definitions(schema);

        // Add common JSON primitives
        self.add_json_primitives();

        // Convert the root schema
        let root_expr = self.convert_schema(schema)?;
        self.declarations
            .insert(0, GbnfDeclaration::new(root.to_string(), root_expr));

        Ok(GbnfGrammar::new(
            std::mem::take(&mut self.declarations),
            root.to_string(),
        ))
    }

    /// Extract $defs or definitions from the schema
    fn extract_definitions(&mut self, schema: &Value) {
        if let Some(obj) = schema.as_object() {
            // JSON Schema draft-07+ uses $defs
            if let Some(defs) = obj.get("$defs").and_then(|v| v.as_object()) {
                for (name, def) in defs {
                    self.definitions
                        .insert(format!("#/$defs/{}", name), def.clone());
                }
            }
            // Older schemas use definitions
            if let Some(defs) = obj.get("definitions").and_then(|v| v.as_object()) {
                for (name, def) in defs {
                    self.definitions
                        .insert(format!("#/definitions/{}", name), def.clone());
                }
            }
        }
    }

    /// Add common JSON primitive rules using the gbnf! macro for cleaner definitions
    fn add_json_primitives(&mut self) {
        use crate::CharacterRange;

        let primitives = gbnf! {
            // Whitespace (optional)
            ws ::= [' ' '\t' '\n' '\r']*

            // JSON number: -?int(.frac)?(e[+-]?int)?
            json-number ::= "-"? json-int json-frac? json-exp?
            json-int ::= "0" | [1-9] [0-9]*
            json-frac ::= "." [0-9]+
            json-exp ::= [e E] ['+' '-']? [0-9]+

            // JSON integer (no fractional part)
            json-integer ::= "-"? ("0" | [1-9] [0-9]*)

            // JSON boolean
            json-boolean ::= "true" | "false"

            // JSON null
            json-null ::= "null"
        };

        self.declarations.extend(primitives.declarations);

        // JSON string rules from llama.cpp docs:
        // json-char ::= [^"\\\x7F\x00-\x1F] | [\\] (["\\bfnrt] | "u" [0-9a-fA-F]{4})
        // json-string ::= "\"" json-char* "\""

        // Build excluded chars: " \ DEL and control chars 0x00-0x1F
        let mut excluded_chars: Vec<char> = vec!['"', '\\', '\x7F'];
        excluded_chars.extend((0x00u8..=0x1Fu8).map(|b| b as char));

        // Hex digits for unicode escapes
        let hex_chars: Vec<char> = "0123456789abcdefABCDEF".chars().collect();

        // json-char ::= [^"\\\x7F\x00-\x1F] | [\\] (["\\bfnrt] | "u" [0-9a-fA-F]{4})
        self.declarations.push(GbnfDeclaration::new(
            "json-char".to_string(),
            Expr::Alternation(vec![
                // [^"\\\x7F\x00-\x1F]
                Expr::CharacterRange(CharacterRange::Set {
                    chars: excluded_chars,
                    negated: true,
                }),
                // [\\] (["\\bfnrt] | "u" [0-9a-fA-F]{4})
                Expr::Sequence(vec![
                    Expr::CharacterRange(CharacterRange::Set {
                        chars: vec!['\\'],
                        negated: false,
                    }),
                    Expr::Group(Box::new(Expr::Alternation(vec![
                        // ["\\bfnrt]
                        Expr::CharacterRange(CharacterRange::Set {
                            chars: vec!['"', '\\', 'b', 'f', 'n', 'r', 't'],
                            negated: false,
                        }),
                        // "u" [0-9a-fA-F]{4}
                        Expr::Sequence(vec![
                            Expr::Characters("u".to_string()),
                            Expr::Quantified {
                                expr: Box::new(Expr::CharacterRange(CharacterRange::Set {
                                    chars: hex_chars,
                                    negated: false,
                                })),
                                quantifier: Quantifier::Exact(4),
                            },
                        ]),
                    ]))),
                ]),
            ]),
        ));

        // json-string ::= "\"" json-char* "\""
        self.declarations.push(GbnfDeclaration::new(
            "json-string".to_string(),
            Expr::Sequence(vec![
                Expr::Characters("\"".to_string()),
                Expr::Quantified {
                    expr: Box::new(Expr::NonTerminal("json-char".to_string())),
                    quantifier: Quantifier::ZeroOrMore,
                },
                Expr::Characters("\"".to_string()),
            ]),
        ));
    }

    /// Generate a unique rule name
    fn next_rule_name(&mut self, prefix: &str) -> String {
        let name = format!("{}-{}", prefix, self.rule_counter);
        self.rule_counter += 1;
        name
    }

    /// Convert a JSON Schema to an expression
    fn convert_schema(&mut self, schema: &Value) -> Result<Expr, JsonSchemaError> {
        // Handle boolean schemas
        if let Some(b) = schema.as_bool() {
            return if b {
                Ok(Expr::NonTerminal("json-value".to_string()))
            } else {
                Err(JsonSchemaError::InvalidSchema(
                    "false schema rejects everything".to_string(),
                ))
            };
        }

        let obj = schema.as_object().ok_or_else(|| {
            JsonSchemaError::InvalidSchema("Schema must be an object or boolean".to_string())
        })?;

        // Handle $ref
        if let Some(ref_value) = obj.get("$ref") {
            return self.convert_ref(ref_value);
        }

        // Handle enum
        if let Some(enum_values) = obj.get("enum") {
            return self.convert_enum(enum_values);
        }

        // Handle const
        if let Some(const_value) = obj.get("const") {
            return self.convert_const(const_value);
        }

        // Handle oneOf
        if let Some(one_of) = obj.get("oneOf") {
            return self.convert_one_of(one_of);
        }

        // Handle anyOf
        if let Some(any_of) = obj.get("anyOf") {
            return self.convert_any_of(any_of);
        }

        // Handle allOf
        if let Some(all_of) = obj.get("allOf") {
            return self.convert_all_of(all_of);
        }

        // Handle type
        if let Some(type_value) = obj.get("type") {
            return self.convert_type(type_value, obj);
        }

        // If no type specified, allow any value
        Ok(Expr::NonTerminal("json-value".to_string()))
    }

    /// Convert a $ref
    fn convert_ref(&mut self, ref_value: &Value) -> Result<Expr, JsonSchemaError> {
        let ref_str = ref_value
            .as_str()
            .ok_or_else(|| JsonSchemaError::InvalidSchema("$ref must be a string".to_string()))?;

        // Check if already converted
        if let Some(rule_name) = self.converted_refs.get(ref_str) {
            return Ok(Expr::NonTerminal(rule_name.clone()));
        }

        // Look up the definition
        let def = self
            .definitions
            .get(ref_str)
            .cloned()
            .ok_or_else(|| JsonSchemaError::UnresolvedRef(ref_str.to_string()))?;

        // Generate a rule name from the ref
        let rule_name = ref_str
            .rsplit('/')
            .next()
            .unwrap_or("ref")
            .to_lowercase()
            .replace('_', "-");
        let rule_name = self.next_rule_name(&rule_name);

        // Mark as converted before recursing to handle circular refs
        self.converted_refs
            .insert(ref_str.to_string(), rule_name.clone());

        // Convert the definition
        let expr = self.convert_schema(&def)?;
        self.declarations
            .push(GbnfDeclaration::new(rule_name.clone(), expr));

        Ok(Expr::NonTerminal(rule_name))
    }

    /// Convert an enum
    fn convert_enum(&mut self, enum_values: &Value) -> Result<Expr, JsonSchemaError> {
        let arr = enum_values
            .as_array()
            .ok_or_else(|| JsonSchemaError::InvalidSchema("enum must be an array".to_string()))?;

        if arr.is_empty() {
            return Err(JsonSchemaError::InvalidSchema(
                "enum cannot be empty".to_string(),
            ));
        }

        let alternatives: Result<Vec<Expr>, _> =
            arr.iter().map(|v| self.convert_const(v)).collect();

        let alternatives = alternatives?;
        if alternatives.len() == 1 {
            Ok(alternatives.into_iter().next().unwrap())
        } else {
            Ok(Expr::Alternation(alternatives))
        }
    }

    /// Convert a const value
    fn convert_const(&mut self, value: &Value) -> Result<Expr, JsonSchemaError> {
        match value {
            Value::Null => Ok(Expr::Characters("null".to_string())),
            Value::Bool(b) => Ok(Expr::Characters(
                if *b { "true" } else { "false" }.to_string(),
            )),
            Value::Number(n) => Ok(Expr::Characters(n.to_string())),
            Value::String(s) => {
                // Need to emit as a JSON string with quotes
                let escaped = escape_json_string(s);
                Ok(Expr::Characters(format!("\"{}\"", escaped)))
            }
            Value::Array(_) | Value::Object(_) => {
                // For complex values, emit the JSON directly
                let json_str = serde_json::to_string(value)
                    .map_err(|e| JsonSchemaError::InvalidJson(e.to_string()))?;
                Ok(Expr::Characters(json_str))
            }
        }
    }

    /// Convert oneOf
    fn convert_one_of(&mut self, one_of: &Value) -> Result<Expr, JsonSchemaError> {
        let arr = one_of
            .as_array()
            .ok_or_else(|| JsonSchemaError::InvalidSchema("oneOf must be an array".to_string()))?;

        let alternatives: Result<Vec<Expr>, _> =
            arr.iter().map(|s| self.convert_schema(s)).collect();

        let alternatives = alternatives?;
        if alternatives.len() == 1 {
            Ok(alternatives.into_iter().next().unwrap())
        } else {
            Ok(Expr::Alternation(alternatives))
        }
    }

    /// Convert anyOf (treated same as oneOf for grammar purposes)
    fn convert_any_of(&mut self, any_of: &Value) -> Result<Expr, JsonSchemaError> {
        self.convert_one_of(any_of)
    }

    /// Convert allOf
    fn convert_all_of(&mut self, all_of: &Value) -> Result<Expr, JsonSchemaError> {
        let arr = all_of
            .as_array()
            .ok_or_else(|| JsonSchemaError::InvalidSchema("allOf must be an array".to_string()))?;

        // For allOf, we need to merge the schemas
        // This is a simplified implementation that only handles object merging
        let mut merged_properties: HashMap<String, Value> = HashMap::new();
        let mut merged_required: Vec<String> = Vec::new();

        for schema in arr {
            if let Some(obj) = schema.as_object() {
                if let Some(props) = obj.get("properties").and_then(|p| p.as_object()) {
                    for (name, prop) in props {
                        merged_properties.insert(name.clone(), prop.clone());
                    }
                }
                if let Some(req) = obj.get("required").and_then(|r| r.as_array()) {
                    for r in req {
                        if let Some(s) = r.as_str() {
                            if !merged_required.contains(&s.to_string()) {
                                merged_required.push(s.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Build a merged schema
        let merged = serde_json::json!({
            "type": "object",
            "properties": merged_properties,
            "required": merged_required
        });

        self.convert_schema(&merged)
    }

    /// Convert based on type
    fn convert_type(
        &mut self,
        type_value: &Value,
        schema: &serde_json::Map<String, Value>,
    ) -> Result<Expr, JsonSchemaError> {
        // Handle array of types
        if let Some(types) = type_value.as_array() {
            let alternatives: Result<Vec<Expr>, _> = types
                .iter()
                .filter_map(|t| t.as_str())
                .map(|t| self.convert_single_type(t, schema))
                .collect();
            let alternatives = alternatives?;
            if alternatives.len() == 1 {
                return Ok(alternatives.into_iter().next().unwrap());
            }
            return Ok(Expr::Alternation(alternatives));
        }

        // Handle single type
        let type_str = type_value.as_str().ok_or_else(|| {
            JsonSchemaError::InvalidSchema("type must be a string or array".to_string())
        })?;

        self.convert_single_type(type_str, schema)
    }

    /// Convert a single type
    fn convert_single_type(
        &mut self,
        type_str: &str,
        schema: &serde_json::Map<String, Value>,
    ) -> Result<Expr, JsonSchemaError> {
        match type_str {
            "string" => self.convert_string_type(schema),
            "number" => Ok(Expr::NonTerminal("json-number".to_string())),
            "integer" => Ok(Expr::NonTerminal("json-integer".to_string())),
            "boolean" => Ok(Expr::NonTerminal("json-boolean".to_string())),
            "null" => Ok(Expr::NonTerminal("json-null".to_string())),
            "array" => self.convert_array_type(schema),
            "object" => self.convert_object_type(schema),
            _ => Err(JsonSchemaError::UnsupportedFeature(format!(
                "Unknown type: {}",
                type_str
            ))),
        }
    }

    /// Convert string type with constraints
    fn convert_string_type(
        &mut self,
        schema: &serde_json::Map<String, Value>,
    ) -> Result<Expr, JsonSchemaError> {
        // Check for pattern constraint
        if let Some(pattern) = schema.get("pattern") {
            return Err(JsonSchemaError::UnsupportedFeature(format!(
                "pattern constraint not yet supported: {:?}",
                pattern
            )));
        }

        // Check for format constraint
        if let Some(format) = schema.get("format").and_then(|f| f.as_str()) {
            return self.convert_string_format(format);
        }

        // Default: any JSON string
        Ok(Expr::NonTerminal("json-string".to_string()))
    }

    /// Convert string format constraints
    fn convert_string_format(&mut self, format: &str) -> Result<Expr, JsonSchemaError> {
        match format {
            "date" => {
                // YYYY-MM-DD
                let rule_name = self.next_rule_name("date");
                let grammar = gbnf! {
                    date ::= "\"" [0-9]{4} "-" [0-9]{2} "-" [0-9]{2} "\""
                };
                // Get the expression from the first declaration
                let expr = grammar.declarations.into_iter().next().unwrap().expr;
                self.declarations
                    .push(GbnfDeclaration::new(rule_name.clone(), expr));
                Ok(Expr::NonTerminal(rule_name))
            }
            "time" => {
                // HH:MM:SS
                let rule_name = self.next_rule_name("time");
                let grammar = gbnf! {
                    time ::= "\"" [0-9]{2} ":" [0-9]{2} ":" [0-9]{2} "\""
                };
                let expr = grammar.declarations.into_iter().next().unwrap().expr;
                self.declarations
                    .push(GbnfDeclaration::new(rule_name.clone(), expr));
                Ok(Expr::NonTerminal(rule_name))
            }
            "date-time" => {
                // ISO 8601: YYYY-MM-DDTHH:MM:SS with optional timezone
                let rule_name = self.next_rule_name("datetime");
                let tz_rule_name = self.next_rule_name("tz");
                let grammar = gbnf! {
                    datetime ::= "\"" [0-9]{4} "-" [0-9]{2} "-" [0-9]{2} "T" [0-9]{2} ":" [0-9]{2} ":" [0-9]{2} tz? "\""
                    tz ::= "Z" | ['+' '-'] [0-9]{2} ":" [0-9]{2}
                };
                let mut decls = grammar.declarations.into_iter();
                let datetime_expr = decls.next().unwrap().expr;
                let tz_expr = decls.next().unwrap().expr;
                self.declarations
                    .push(GbnfDeclaration::new(tz_rule_name, tz_expr));
                self.declarations
                    .push(GbnfDeclaration::new(rule_name.clone(), datetime_expr));
                Ok(Expr::NonTerminal(rule_name))
            }
            "email" | "uri" | "uuid" => {
                // For now, just use generic string
                // These could be implemented with more specific patterns
                Ok(Expr::NonTerminal("json-string".to_string()))
            }
            _ => {
                // Unknown format, fall back to generic string
                Ok(Expr::NonTerminal("json-string".to_string()))
            }
        }
    }

    /// Convert array type
    ///
    /// Supports:
    /// - `items`: schema for all array elements (homogeneous array)
    /// - `prefixItems`: schemas for positional elements (tuple-like)
    /// - Both: prefixItems first, then items for additional elements
    fn convert_array_type(
        &mut self,
        schema: &serde_json::Map<String, Value>,
    ) -> Result<Expr, JsonSchemaError> {
        let prefix_items = schema.get("prefixItems").and_then(|p| p.as_array());
        let items_schema = schema.get("items");

        match (prefix_items, items_schema) {
            // Only prefixItems: tuple with fixed elements
            (Some(prefix), None) => self.convert_tuple_array(prefix, None),

            // Both prefixItems and items: tuple prefix + additional items
            (Some(prefix), Some(items)) => self.convert_tuple_array(prefix, Some(items)),

            // Only items or neither: homogeneous array
            _ => self.convert_homogeneous_array(items_schema),
        }
    }

    /// Convert a homogeneous array (all elements same type)
    fn convert_homogeneous_array(
        &mut self,
        items_schema: Option<&Value>,
    ) -> Result<Expr, JsonSchemaError> {
        let items_expr = if let Some(items) = items_schema {
            self.convert_schema(items)?
        } else {
            Expr::NonTerminal("json-value".to_string())
        };

        let item_rule = self.next_rule_name("item");
        self.declarations
            .push(GbnfDeclaration::new(item_rule.clone(), items_expr));

        let rule_name = self.next_rule_name("array");
        let expr = Expr::Sequence(vec![
            Expr::Characters("[".to_string()),
            Expr::NonTerminal("ws".to_string()),
            Expr::Quantified {
                expr: Box::new(Expr::Sequence(vec![
                    Expr::NonTerminal(item_rule.clone()),
                    Expr::Quantified {
                        expr: Box::new(Expr::Sequence(vec![
                            Expr::NonTerminal("ws".to_string()),
                            Expr::Characters(",".to_string()),
                            Expr::NonTerminal("ws".to_string()),
                            Expr::NonTerminal(item_rule),
                        ])),
                        quantifier: Quantifier::ZeroOrMore,
                    },
                ])),
                quantifier: Quantifier::Optional,
            },
            Expr::NonTerminal("ws".to_string()),
            Expr::Characters("]".to_string()),
        ]);

        self.declarations
            .push(GbnfDeclaration::new(rule_name.clone(), expr));
        Ok(Expr::NonTerminal(rule_name))
    }

    /// Convert a tuple array (prefixItems with optional trailing items)
    fn convert_tuple_array(
        &mut self,
        prefix_items: &[Value],
        additional_items: Option<&Value>,
    ) -> Result<Expr, JsonSchemaError> {
        if prefix_items.is_empty() {
            // No prefix items, fall back to homogeneous array
            return self.convert_homogeneous_array(additional_items);
        }

        // Convert each prefix item schema to a rule
        let mut prefix_rules: Vec<String> = Vec::new();
        for (i, item_schema) in prefix_items.iter().enumerate() {
            let item_expr = self.convert_schema(item_schema)?;
            let rule_name = self.next_rule_name(&format!("tuple-item-{}", i));
            self.declarations
                .push(GbnfDeclaration::new(rule_name.clone(), item_expr));
            prefix_rules.push(rule_name);
        }

        // Build the tuple expression: "[" ws item0 ws "," ws item1 ... "]"
        let mut parts: Vec<Expr> = vec![
            Expr::Characters("[".to_string()),
            Expr::NonTerminal("ws".to_string()),
        ];

        // Add first prefix item
        parts.push(Expr::NonTerminal(prefix_rules[0].clone()));

        // Add remaining prefix items with comma separators
        for rule in prefix_rules.iter().skip(1) {
            parts.push(Expr::NonTerminal("ws".to_string()));
            parts.push(Expr::Characters(",".to_string()));
            parts.push(Expr::NonTerminal("ws".to_string()));
            parts.push(Expr::NonTerminal(rule.clone()));
        }

        // Add additional items if specified (but not if items: false)
        // items: false means "no additional items allowed" which is the default behavior
        if let Some(items_schema) = additional_items {
            // items: false means no additional items - same as None
            if items_schema.as_bool() != Some(false) {
                let items_expr = self.convert_schema(items_schema)?;
                let items_rule = self.next_rule_name("tuple-rest");
                self.declarations
                    .push(GbnfDeclaration::new(items_rule.clone(), items_expr));

                // (ws "," ws item)*
                parts.push(Expr::Quantified {
                    expr: Box::new(Expr::Sequence(vec![
                        Expr::NonTerminal("ws".to_string()),
                        Expr::Characters(",".to_string()),
                        Expr::NonTerminal("ws".to_string()),
                        Expr::NonTerminal(items_rule),
                    ])),
                    quantifier: Quantifier::ZeroOrMore,
                });
            }
        }

        parts.push(Expr::NonTerminal("ws".to_string()));
        parts.push(Expr::Characters("]".to_string()));

        let rule_name = self.next_rule_name("tuple");
        self.declarations.push(GbnfDeclaration::new(
            rule_name.clone(),
            Expr::Sequence(parts),
        ));
        Ok(Expr::NonTerminal(rule_name))
    }

    /// Convert object type
    ///
    /// additionalProperties handling:
    /// - `false` or absent: no additional properties allowed
    /// - `{schema}`: additional properties with constrained values (fixed ordering: defined props first)
    fn convert_object_type(
        &mut self,
        schema: &serde_json::Map<String, Value>,
    ) -> Result<Expr, JsonSchemaError> {
        let properties = schema.get("properties").and_then(|p| p.as_object());

        let required: Vec<&str> = schema
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        // additionalProperties: only support false or a schema object
        let additional_schema = schema.get("additionalProperties").filter(|v| v.is_object());

        // Handle case with no defined properties
        let properties = match properties {
            Some(p) if !p.is_empty() => p,
            _ => {
                return self.convert_object_only_additional(additional_schema);
            }
        };

        // Build property value rules
        let mut prop_rules: Vec<(String, String, bool)> = Vec::new();

        for (prop_name, prop_schema) in properties {
            let prop_expr = self.convert_schema(prop_schema)?;
            let rule_name = self.next_rule_name(&format!("prop-{}", prop_name.replace('_', "-")));
            self.declarations
                .push(GbnfDeclaration::new(rule_name.clone(), prop_expr));
            prop_rules.push((
                prop_name.clone(),
                rule_name,
                required.contains(&prop_name.as_str()),
            ));
        }

        let required_props: Vec<_> = prop_rules.iter().filter(|(_, _, r)| *r).collect();
        let optional_props: Vec<_> = prop_rules.iter().filter(|(_, _, r)| !*r).collect();

        // Build object parts
        let mut obj_parts: Vec<Expr> = vec![
            Expr::Characters("{".to_string()),
            Expr::NonTerminal("ws".to_string()),
        ];

        let mut has_content = false;

        // Required properties
        for (prop_name, prop_rule_name, _) in &required_props {
            if has_content {
                obj_parts.extend(Self::comma_separator());
            }
            has_content = true;
            obj_parts.extend(Self::property_kv(prop_name, prop_rule_name));
        }

        // Optional defined properties
        for (prop_name, prop_rule_name, _) in &optional_props {
            let opt_rule_name =
                self.next_rule_name(&format!("opt-{}", prop_name.replace('_', "-")));

            let mut opt_parts = if has_content {
                Self::comma_separator()
            } else {
                vec![]
            };
            opt_parts.extend(Self::property_kv(prop_name, prop_rule_name));

            self.declarations.push(GbnfDeclaration::new(
                opt_rule_name.clone(),
                Expr::Sequence(opt_parts),
            ));

            obj_parts.push(Expr::Quantified {
                expr: Box::new(Expr::NonTerminal(opt_rule_name)),
                quantifier: Quantifier::Optional,
            });
        }

        // Additional properties
        if let Some(add_schema) = additional_schema {
            let add_prop_rule = self.create_additional_prop_rule(add_schema)?;

            // (ws "," ws additional-prop)*
            obj_parts.push(Expr::Quantified {
                expr: Box::new(Expr::Sequence(vec![
                    Expr::NonTerminal("ws".to_string()),
                    Expr::Characters(",".to_string()),
                    Expr::NonTerminal("ws".to_string()),
                    Expr::NonTerminal(add_prop_rule),
                ])),
                quantifier: Quantifier::ZeroOrMore,
            });
        }

        obj_parts.push(Expr::NonTerminal("ws".to_string()));
        obj_parts.push(Expr::Characters("}".to_string()));

        let rule_name = self.next_rule_name("object");
        self.declarations.push(GbnfDeclaration::new(
            rule_name.clone(),
            Expr::Sequence(obj_parts),
        ));

        Ok(Expr::NonTerminal(rule_name))
    }

    /// Create a rule for additional properties: json-string ws ":" ws <value-type>
    fn create_additional_prop_rule(
        &mut self,
        value_schema: &Value,
    ) -> Result<String, JsonSchemaError> {
        let value_expr = self.convert_schema(value_schema)?;
        let value_rule = self.next_rule_name("addl-value");
        self.declarations
            .push(GbnfDeclaration::new(value_rule.clone(), value_expr));

        let prop_rule = self.next_rule_name("addl-prop");
        let prop_expr = Expr::Sequence(vec![
            Expr::NonTerminal("json-string".to_string()),
            Expr::NonTerminal("ws".to_string()),
            Expr::Characters(":".to_string()),
            Expr::NonTerminal("ws".to_string()),
            Expr::NonTerminal(value_rule),
        ]);
        self.declarations
            .push(GbnfDeclaration::new(prop_rule.clone(), prop_expr));

        Ok(prop_rule)
    }

    /// Convert object with no defined properties, only additionalProperties
    fn convert_object_only_additional(
        &mut self,
        additional_schema: Option<&Value>,
    ) -> Result<Expr, JsonSchemaError> {
        match additional_schema {
            Some(add_schema) => {
                let add_prop_rule = self.create_additional_prop_rule(add_schema)?;
                let rule_name = self.next_rule_name("object");

                // Pattern: { ws (add-prop (ws "," ws add-prop)*)? ws }
                let obj_expr = Expr::Sequence(vec![
                    Expr::Characters("{".to_string()),
                    Expr::NonTerminal("ws".to_string()),
                    Expr::Quantified {
                        expr: Box::new(Expr::Sequence(vec![
                            Expr::NonTerminal(add_prop_rule.clone()),
                            Expr::Quantified {
                                expr: Box::new(Expr::Sequence(vec![
                                    Expr::NonTerminal("ws".to_string()),
                                    Expr::Characters(",".to_string()),
                                    Expr::NonTerminal("ws".to_string()),
                                    Expr::NonTerminal(add_prop_rule),
                                ])),
                                quantifier: Quantifier::ZeroOrMore,
                            },
                        ])),
                        quantifier: Quantifier::Optional,
                    },
                    Expr::NonTerminal("ws".to_string()),
                    Expr::Characters("}".to_string()),
                ]);

                self.declarations
                    .push(GbnfDeclaration::new(rule_name.clone(), obj_expr));
                Ok(Expr::NonTerminal(rule_name))
            }
            None => {
                // Empty object only: { ws }
                let grammar = gbnf! {
                    empty ::= "{" ws "}"
                };
                Ok(grammar.declarations.into_iter().next().unwrap().expr)
            }
        }
    }

    /// Helper: comma separator sequence
    fn comma_separator() -> Vec<Expr> {
        vec![
            Expr::NonTerminal("ws".to_string()),
            Expr::Characters(",".to_string()),
            Expr::NonTerminal("ws".to_string()),
        ]
    }

    /// Helper: property key-value pair
    fn property_kv(prop_name: &str, value_rule: &str) -> Vec<Expr> {
        vec![
            Expr::Characters(format!("\"{}\"", escape_json_string(prop_name))),
            Expr::NonTerminal("ws".to_string()),
            Expr::Characters(":".to_string()),
            Expr::NonTerminal("ws".to_string()),
            Expr::NonTerminal(value_rule.to_string()),
        ]
    }
}

impl Default for JsonSchemaConverter {
    fn default() -> Self {
        Self::new()
    }
}

/// Escape a string for use in JSON
fn escape_json_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

/// Trait for types that can be converted to a JSON Schema value
pub trait IntoJsonSchema {
    fn into_schema(self) -> Result<Value, JsonSchemaError>;
}

impl IntoJsonSchema for &str {
    fn into_schema(self) -> Result<Value, JsonSchemaError> {
        serde_json::from_str(self).map_err(|e| JsonSchemaError::InvalidJson(e.to_string()))
    }
}

impl IntoJsonSchema for String {
    fn into_schema(self) -> Result<Value, JsonSchemaError> {
        serde_json::from_str(&self).map_err(|e| JsonSchemaError::InvalidJson(e.to_string()))
    }
}

impl IntoJsonSchema for Value {
    fn into_schema(self) -> Result<Value, JsonSchemaError> {
        Ok(self)
    }
}

impl IntoJsonSchema for &Value {
    fn into_schema(self) -> Result<Value, JsonSchemaError> {
        Ok(self.clone())
    }
}

/// Convert a JSON Schema to a GBNF Grammar
///
/// Accepts `&str`, `String`, `Value`, or `&Value`.
///
/// # Example
///
/// ```
/// use gbnf::json::json_schema_to_grammar;
///
/// // From string
/// let grammar = json_schema_to_grammar(r#"{"type": "string"}"#, "root").unwrap();
///
/// // From serde_json::Value
/// let value = serde_json::json!({"type": "integer"});
/// let grammar = json_schema_to_grammar(value, "root").unwrap();
///
/// // From serde_json::Value converted to String
/// let value = serde_json::json!({"type": "integer"});
/// let grammar = json_schema_to_grammar(value.to_string(), "root").unwrap();
/// ```
pub fn json_schema_to_grammar(
    schema: impl IntoJsonSchema,
    root: &str,
) -> Result<GbnfGrammar, JsonSchemaError> {
    let value = schema.into_schema()?;
    if !jsonschema::meta::is_valid(&value) {
        return Err(JsonSchemaError::InvalidSchema(format!(
            "Not a valid json schema: {}",
            value
        )));
    };
    let mut converter = JsonSchemaConverter::new();
    converter.convert(&value, root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_string() {
        let schema = r#"{"type": "string"}"#;
        let grammar = json_schema_to_grammar(schema, "root").unwrap();
        assert!(grammar.as_str().contains("root ::= json-string"));
    }

    #[test]
    fn test_simple_integer() {
        let schema = r#"{"type": "integer"}"#;
        let grammar = json_schema_to_grammar(schema, "root").unwrap();
        assert!(grammar.as_str().contains("root ::= json-integer"));
    }

    #[test]
    fn test_enum() {
        let schema = r#"{"enum": ["red", "green", "blue"]}"#;
        let grammar = json_schema_to_grammar(schema, "root").unwrap();
        let gbnf = grammar.as_str();
        // The grammar escapes quotes inside strings, so "red" becomes \"red\"
        assert!(gbnf.contains(r#"\"red\""#));
        assert!(gbnf.contains(r#"\"green\""#));
        assert!(gbnf.contains(r#"\"blue\""#));
    }

    #[test]
    fn test_object_with_properties() {
        let schema = r#"{
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            },
            "required": ["name"]
        }"#;
        let grammar = json_schema_to_grammar(schema, "root").unwrap();
        let gbnf = grammar.as_str();
        // Property names are escaped in the grammar
        assert!(gbnf.contains(r#"\"name\""#));
        assert!(gbnf.contains(r#"\"age\""#));
    }

    #[test]
    fn test_nested_objects() {
        let schema = r#"{
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"},
                "inner_object": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "integer"},
                        "age": {"type": "string"}
                    },
                    "required": ["name", "age"]
                }
            },
            "required": ["name", "age", "inner_object"]
        }"#;
        let grammar = json_schema_to_grammar(schema, "root").unwrap();
        let gbnf = grammar.as_str();

        // Should have the outer object with all three properties
        assert!(gbnf.contains(r#"\"name\""#));
        assert!(gbnf.contains(r#"\"age\""#));
        assert!(gbnf.contains(r#"\"inner_object\""#));

        // Inner object should have its own property rules with different types
        // (inner "name" is integer, inner "age" is string — reversed from outer)
        assert!(gbnf.contains("prop-name-"));
        assert!(gbnf.contains("prop-age-"));

        // Should have two object rules (outer and inner)
        assert!(gbnf.contains("object-"));

        // The inner object's property types should differ from outer
        // outer: name=string, age=integer; inner: name=integer, age=string
        // So we should see both json-string and json-integer referenced
        assert!(gbnf.contains("json-string"));
        assert!(gbnf.contains("json-integer"));
    }

    #[test]
    fn test_array_of_strings() {
        let schema = r#"{
            "type": "array",
            "items": {"type": "string"}
        }"#;
        let grammar = json_schema_to_grammar(schema, "root").unwrap();
        let gbnf = grammar.as_str();
        assert!(gbnf.contains("["));
        assert!(gbnf.contains("]"));
    }

    #[test]
    fn test_nested_arrays_matrix() {
        let schema = r#"{"type":"object","properties":{"listOfMatrices":{"type":"array","items":{"type":"array","items":{"type":"array","items":{"type":"number"}}}}},"required":["listOfMatrices"],"additionalProperties":false}"#;
        let grammar = json_schema_to_grammar(schema, "root").unwrap();
        let gbnf = grammar.as_str();

        // Should have the listOfMatrices property
        assert!(gbnf.contains(r#"\"listOfMatrices\""#));
        // Should reference json-number for the innermost array items
        assert!(gbnf.contains("json-number"));
        // Should have nested array structures
        assert!(gbnf.contains("item-"));
        assert!(gbnf.contains("array-"));
    }

    #[test]
    fn test_additional_properties_schema() {
        // Object with one required prop and additionalProperties constrained to integers
        let schema = r#"{
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            },
            "required": ["name"],
            "additionalProperties": {"type": "integer"}
        }"#;
        let grammar = json_schema_to_grammar(schema, "root").unwrap();
        let gbnf = grammar.as_str();
        eprintln!("Generated grammar:\n{}", gbnf);

        // Should have the name property
        assert!(gbnf.contains(r#"\"name\""#));
        // Should have additional property rule referencing json-integer
        assert!(gbnf.contains("addl-prop"));
        assert!(gbnf.contains("json-integer"));
    }

    #[test]
    fn test_additional_properties_only() {
        // Object with no defined properties, only additionalProperties
        let schema = r#"{
            "type": "object",
            "additionalProperties": {"type": "boolean"}
        }"#;
        let grammar = json_schema_to_grammar(schema, "root").unwrap();
        let gbnf = grammar.as_str();
        eprintln!("Generated grammar:\n{}", gbnf);

        // Should have additional property rule referencing json-boolean
        assert!(gbnf.contains("addl-prop"));
        assert!(gbnf.contains("json-boolean"));
    }

    #[test]
    fn test_prefix_items_tuple() {
        // Tuple: [string, integer, boolean]
        let schema = r#"{
            "type": "array",
            "prefixItems": [
                {"type": "string"},
                {"type": "integer"},
                {"type": "boolean"}
            ]
        }"#;
        let grammar = json_schema_to_grammar(schema, "root").unwrap();
        let gbnf = grammar.as_str();
        eprintln!("Generated grammar:\n{}", gbnf);

        // Should have tuple item rules
        assert!(gbnf.contains("tuple-item-0"));
        assert!(gbnf.contains("tuple-item-1"));
        assert!(gbnf.contains("tuple-item-2"));
        // Should reference the proper types
        assert!(gbnf.contains("json-string"));
        assert!(gbnf.contains("json-integer"));
        assert!(gbnf.contains("json-boolean"));
    }

    #[test]
    fn test_prefix_items_with_additional() {
        // Tuple with additional items: [string, integer, ...numbers]
        let schema = r#"{
            "type": "array",
            "prefixItems": [
                {"type": "string"},
                {"type": "integer"}
            ],
            "items": {"type": "number"}
        }"#;
        let grammar = json_schema_to_grammar(schema, "root").unwrap();
        let gbnf = grammar.as_str();
        eprintln!("Generated grammar:\n{}", gbnf);

        // Should have tuple item rules for prefix
        assert!(gbnf.contains("tuple-item-0"));
        assert!(gbnf.contains("tuple-item-1"));
        // Should have a rule for additional items
        assert!(gbnf.contains("tuple-rest"));
        // Should reference json-number for additional items
        assert!(gbnf.contains("json-number"));
    }

    #[test]
    fn test_prefix_items_no_additional() {
        // Tuple with items: false means no additional items allowed
        let schema = r#"{
            "type": "array",
            "prefixItems": [
                {"type": "string"},
                {"type": "integer"}
            ],
            "items": false
        }"#;
        let grammar = json_schema_to_grammar(schema, "root").unwrap();
        let gbnf = grammar.as_str();
        eprintln!("Generated grammar:\n{}", gbnf);

        // Should have tuple item rules for prefix
        assert!(gbnf.contains("tuple-item-0"));
        assert!(gbnf.contains("tuple-item-1"));
        // Should NOT have tuple-rest since items: false
        assert!(!gbnf.contains("tuple-rest"));
    }

    #[test]
    fn test_nonsense_schema() {
        let schema = r#"
        { "type" : "string", "items" : "integer", "text" : "hello"}
        "#;

        let grammar = json_schema_to_grammar(schema, "root");

        assert!(matches!(grammar, Err(JsonSchemaError::InvalidSchema(_))));
    }

    #[test]
    fn test_complex_schema_with_refs() {
        let schema = r##"{
          "$defs": {
            "FooBar": {
              "properties": {
                "count": {
                  "title": "Count",
                  "type": "integer"
                },
                "size": {
                  "anyOf": [
                    { "type": "number" },
                    { "type": "null" }
                  ],
                  "default": null,
                  "title": "Size"
                }
              },
              "required": ["count"],
              "title": "FooBar",
              "type": "object"
            },
            "Gender": {
              "enum": ["male", "female", "other", "not_given"],
              "title": "Gender",
              "type": "string"
            }
          },
          "description": "This is the description of the main model",
          "properties": {
            "foo_bar": {
              "$ref": "#/$defs/FooBar"
            },
            "Gender": {
              "anyOf": [
                { "$ref": "#/$defs/Gender" },
                { "type": "null" }
              ],
              "default": null
            },
            "snap": {
              "default": 42,
              "description": "this is the value of snap",
              "exclusiveMaximum": 50,
              "exclusiveMinimum": 30,
              "title": "The Snap",
              "type": "integer"
            }
          },
          "required": ["foo_bar"],
          "title": "Main",
          "type": "object"
        }"##;

        let grammar = json_schema_to_grammar(schema, "root").unwrap();
        let gbnf = grammar.as_str();
        eprintln!("Generated grammar:\n{}", gbnf);

        // Should have the foo_bar property (required)
        assert!(gbnf.contains(r#"\"foo_bar\""#));
        // Should reference FooBar definition
        assert!(gbnf.contains("foobar-"));
        // Should have Gender enum values
        assert!(gbnf.contains(r#"\"male\""#));
        assert!(gbnf.contains(r#"\"female\""#));
        // Should have count and size properties from FooBar
        assert!(gbnf.contains(r#"\"count\""#));
        assert!(gbnf.contains(r#"\"size\""#));
        // Should have json-integer for snap
        assert!(gbnf.contains("json-integer"));
        // Should have json-null for nullable types
        assert!(gbnf.contains("json-null"));
    }
}
