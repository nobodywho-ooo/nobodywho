//! GBNF to Lark grammar converter.
//!
//! Converts llama.cpp GBNF grammar syntax to Lark syntax for use with llguidance.
//! Ported from the Python reference at guidance-ai/llguidance.

use std::collections::{HashMap, HashSet};

// ===== Error =====

#[derive(Debug)]
pub enum GbnfToLarkError {
    ParseError { position: String, message: String },
    ResolutionError(String),
}

impl std::fmt::Display for GbnfToLarkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseError { position, message } => write!(f, "{} at {}", message, position),
            Self::ResolutionError(msg) => write!(f, "Resolution error: {}", msg),
        }
    }
}

impl std::error::Error for GbnfToLarkError {}

// ===== Internal AST =====

#[derive(Debug, Clone)]
enum Node {
    Literal(String),
    Regex(String),
    RuleRef(String),
    /// A llama.cpp GBNF token reference (`<name>` or `<[42]>`), optionally
    /// negated (`!<name>`). Maps to Lark `<name>` / `~<name>` syntax.
    SpecialToken {
        name: String,
        negated: bool,
    },
    Repetition {
        node: Box<Node>,
        min: u32,
        max: Option<u32>,
    },
    Sequence(Vec<Node>),
    Alternative(Vec<Node>),
}

impl Node {
    fn simplify(self) -> Self {
        match self {
            Node::Sequence(nodes) => {
                let nodes: Vec<_> = nodes.into_iter().map(Self::simplify).collect();
                if nodes.len() == 1 {
                    nodes.into_iter().next().unwrap()
                } else {
                    Node::Sequence(nodes)
                }
            }
            Node::Alternative(alts) => {
                let alts: Vec<_> = alts.into_iter().map(Self::simplify).collect();
                if alts.len() == 1 {
                    alts.into_iter().next().unwrap()
                } else {
                    Node::Alternative(alts)
                }
            }
            Node::Repetition { node, min, max } => Node::Repetition {
                node: Box::new(node.simplify()),
                min,
                max,
            },
            other => other,
        }
    }

    fn is_atomic(&self) -> bool {
        !matches!(self, Node::Sequence(_) | Node::Alternative(_))
    }

    fn is_terminal(&self, terminal_names: &HashSet<String>) -> bool {
        match self {
            Node::Literal(_) | Node::Regex(_) => true,
            // Special tokens resolve to specific vocab token IDs at sampler-
            // init time. llguidance forbids them inside Lark TERMINALS (the
            // lexer operates on bytes, not token IDs), so any rule that
            // references one must stay a non-terminal.
            Node::SpecialToken { .. } => false,
            Node::RuleRef(name) => terminal_names.contains(name),
            Node::Repetition { node, .. } => node.is_terminal(terminal_names),
            Node::Sequence(nodes) | Node::Alternative(nodes) => {
                nodes.iter().all(|n| n.is_terminal(terminal_names))
            }
        }
    }

    fn rename_refs(&mut self, old: &str, new: &str) {
        match self {
            Node::RuleRef(name) if name == old => *name = new.to_string(),
            Node::RuleRef(_) | Node::Literal(_) | Node::Regex(_) | Node::SpecialToken { .. } => {}
            Node::Repetition { node, .. } => node.rename_refs(old, new),
            Node::Sequence(nodes) | Node::Alternative(nodes) => {
                for n in nodes.iter_mut() {
                    n.rename_refs(old, new);
                }
            }
        }
    }

    fn apply_rename_map(&mut self, map: &HashMap<String, String>) {
        match self {
            Node::RuleRef(name) => {
                if let Some(new) = map.get(name.as_str()) {
                    *name = new.clone();
                }
            }
            Node::Literal(_) | Node::Regex(_) | Node::SpecialToken { .. } => {}
            Node::Repetition { node, .. } => node.apply_rename_map(map),
            Node::Sequence(nodes) | Node::Alternative(nodes) => {
                for n in nodes.iter_mut() {
                    n.apply_rename_map(map);
                }
            }
        }
    }

    fn validate_refs(&self, known: &HashSet<String>) -> Result<(), GbnfToLarkError> {
        match self {
            Node::RuleRef(name) => {
                if !known.contains(name) {
                    return Err(GbnfToLarkError::ResolutionError(format!(
                        "Rule '{}' not found",
                        name
                    )));
                }
            }
            Node::Literal(_) | Node::Regex(_) | Node::SpecialToken { .. } => {}
            Node::Repetition { node, .. } => node.validate_refs(known)?,
            Node::Sequence(nodes) | Node::Alternative(nodes) => {
                for n in nodes {
                    n.validate_refs(known)?;
                }
            }
        }
        Ok(())
    }

    /// Render to Lark. All `RuleRef` names must already be final lark names.
    fn to_lark(&self) -> String {
        match self {
            Node::Literal(s) => format!("\"{}\"", s),
            Node::Regex(rx) => format!("/{}/", rx),
            Node::RuleRef(name) => name.clone(),
            Node::SpecialToken { name, negated } => {
                if *negated {
                    format!("~<{}>", name)
                } else {
                    format!("<{}>", name)
                }
            }
            Node::Repetition { node, min, max } => {
                let inner = node.to_lark();
                let inner = if node.is_atomic() {
                    inner
                } else {
                    format!("({})", inner)
                };
                match (min, max) {
                    (0, None) => format!("{}*", inner),
                    (1, None) => format!("{}+", inner),
                    (0, Some(1)) => format!("{}?", inner),
                    (n, Some(m)) if n == m => format!("{}{{{}}}", inner, n),
                    (n, Some(m)) => format!("{}{{{},{}}}", inner, n, m),
                    (n, None) => format!("{}{{{},}}", inner, n),
                }
            }
            Node::Sequence(nodes) => {
                if nodes.is_empty() {
                    "\"\"".to_string()
                } else {
                    nodes
                        .iter()
                        .map(|n| n.to_lark())
                        .collect::<Vec<_>>()
                        .join(" ")
                }
            }
            Node::Alternative(alts) => format!(
                "({})",
                alts.iter()
                    .map(|n| n.to_lark())
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
        }
    }

    /// Top-level render: alternatives use multiline `\n     | ` separator.
    fn to_lark_top(&self) -> String {
        match self {
            Node::Alternative(alts) => alts
                .iter()
                .map(|n| n.to_lark())
                .collect::<Vec<_>>()
                .join("\n     | "),
            _ => self.to_lark(),
        }
    }
}

// ===== Rule =====

struct Rule {
    name: String,
    body: Node,
    comment: String,
    is_terminal: bool,
    order: usize,
}

// ===== Parser =====

struct Parser {
    chars: Vec<char>,
    pos: usize,
    curr_comment: String,
}

impl Parser {
    fn new(text: &str) -> Self {
        Self {
            chars: text.chars().collect(),
            pos: 0,
            curr_comment: String::new(),
        }
    }

    fn current(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self, n: usize) {
        self.pos += n;
    }

    fn position_str(&self) -> String {
        let line_no = self.chars[..self.pos]
            .iter()
            .filter(|&&c| c == '\n')
            .count()
            + 1;
        let ctx_start = self.pos.saturating_sub(20);
        let pref: String = self.chars[ctx_start..self.pos].iter().collect();
        let end = (self.pos + 20).min(self.chars.len());
        let suff: String = self.chars[self.pos..end].iter().collect();
        format!("line {}, {:?} ^ {:?}", line_no, pref, suff)
    }

    fn err(&self, msg: &str) -> GbnfToLarkError {
        GbnfToLarkError::ParseError {
            position: self.position_str(),
            message: msg.to_string(),
        }
    }

    fn is_word_char(c: char) -> bool {
        c.is_alphanumeric() || c == '-' || c == '_'
    }

    fn skip_space(&mut self, allow_newlines: bool) {
        loop {
            match self.current() {
                Some(' ') | Some('\t') => self.advance(1),
                Some('\r') | Some('\n') if allow_newlines => self.skip_newline(),
                Some('#') => {
                    self.advance(1);
                    let mut cmt = "//".to_string();
                    while let Some(c) = self.current() {
                        if c == '\r' || c == '\n' {
                            break;
                        }
                        cmt.push(c);
                        self.advance(1);
                    }
                    self.curr_comment.push_str(&cmt);
                    self.curr_comment.push('\n');
                }
                _ => break,
            }
        }
    }

    fn skip_newline(&mut self) {
        if self.current() == Some('\r') {
            self.advance(1);
            if self.current() == Some('\n') {
                self.advance(1);
            }
        } else if self.current() == Some('\n') {
            self.advance(1);
        }
    }

    fn parse_char(&mut self) -> Result<String, GbnfToLarkError> {
        if self.current() == Some('\\') {
            self.advance(1);
            match self.current() {
                None => return Err(self.err("Incomplete escape sequence")),
                Some(c @ ('"' | '\\' | '[' | ']' | 'n' | 'r' | 't')) => {
                    self.advance(1);
                    return Ok(format!("\\{}", c));
                }
                Some('x') => {
                    self.advance(1);
                    let hex: String = self.chars[self.pos..].iter().take(2).collect();
                    if hex.len() != 2 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                        return Err(self.err(&format!("Invalid \\x escape: \\x{}", hex)));
                    }
                    self.advance(2);
                    return Ok(format!("\\x{}", hex));
                }
                Some('u') => {
                    self.advance(1);
                    let hex: String = self.chars[self.pos..].iter().take(4).collect();
                    if hex.len() != 4 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                        return Err(self.err(&format!("Invalid \\u escape: \\u{}", hex)));
                    }
                    self.advance(4);
                    let stripped = hex.trim_start_matches('0');
                    let stripped = if stripped.is_empty() { "0" } else { stripped };
                    return Ok(format!("\\u{}", stripped));
                }
                Some('U') => {
                    self.advance(1);
                    let hex: String = self.chars[self.pos..].iter().take(8).collect();
                    if hex.len() != 8 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                        return Err(self.err(&format!("Invalid \\U escape: \\U{}", hex)));
                    }
                    self.advance(8);
                    let stripped = hex.trim_start_matches('0');
                    let stripped = if stripped.is_empty() { "0" } else { stripped };
                    return Ok(format!("\\U{}", stripped));
                }
                Some(c) => return Err(self.err(&format!("Invalid escape \\{}", c))),
            }
        }
        match self.current() {
            None => Err(self.err("Unexpected end of input")),
            Some(c) => {
                self.advance(1);
                Ok(c.to_string())
            }
        }
    }

    fn parse_char_class(&mut self) -> Result<Node, GbnfToLarkError> {
        if self.current() != Some('[') {
            return Err(self.err("Expected '['"));
        }
        let mut r = String::from("[");
        self.advance(1);
        loop {
            let c = self.parse_char()?;
            match c.as_str() {
                "/" => r.push_str("\\/"),
                "[" => r.push_str("\\["),
                other => r.push_str(other),
            }
            if c == "]" {
                break;
            }
        }
        Ok(Node::Regex(r))
    }

    /// Parse a special-token reference: `<name>` (e.g. `<|"|>`, `<[42]>`).
    ///
    /// Both llama.cpp GBNF and Lark/llguidance use this syntax to denote a
    /// specific vocab token — the bytes between the angle brackets are the
    /// token's name in the model's vocab (or `[N]` / `[N-M]` for token-id
    /// ranges). At sampler-init time it's resolved to one (or a range of)
    /// token IDs and used as a single-token constraint. We just pass the
    /// inner bytes through unchanged. `negated` reflects a leading `!`
    /// already consumed by the caller (rendered as `~<name>` in Lark).
    fn parse_special_token(&mut self, negated: bool) -> Result<Node, GbnfToLarkError> {
        if self.current() != Some('<') {
            return Err(self.err("Expected '<'"));
        }
        self.advance(1);
        let start = self.pos;
        while let Some(c) = self.current() {
            if c == '>' {
                break;
            }
            if c == '<' || c.is_whitespace() {
                return Err(self.err("Invalid character in token reference"));
            }
            self.advance(1);
        }
        if self.current() != Some('>') {
            return Err(self.err("Unterminated token reference (expected '>')"));
        }
        let name: String = self.chars[start..self.pos].iter().collect();
        self.advance(1);
        if name.is_empty() {
            return Err(self.err("Empty token reference"));
        }
        Ok(Node::SpecialToken { name, negated })
    }

    fn parse_literal(&mut self) -> Result<Node, GbnfToLarkError> {
        if self.current() != Some('"') {
            return Err(self.err("Expected '\"'"));
        }
        self.advance(1);
        let mut r = String::new();
        loop {
            let c = self.parse_char()?;
            if c == "\"" {
                break;
            }
            r.push_str(&c);
        }
        Ok(Node::Literal(r))
    }

    fn parse_name(&mut self) -> Result<String, GbnfToLarkError> {
        let start = self.pos;
        while self.current().is_some_and(Self::is_word_char) {
            self.advance(1);
        }
        if self.pos == start {
            return Err(self.err("Expected name"));
        }
        Ok(self.chars[start..self.pos].iter().collect())
    }

    fn parse_int(&mut self) -> Result<u32, GbnfToLarkError> {
        let start = self.pos;
        while self.current().is_some_and(|c| c.is_ascii_digit()) {
            self.advance(1);
        }
        if self.pos == start {
            return Err(self.err("Expected integer"));
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        s.parse().map_err(|_| self.err("Integer overflow"))
    }

    fn parse_rule(&mut self) -> Result<Rule, GbnfToLarkError> {
        let name = self.parse_name()?;
        self.skip_space(false);

        let next3: String = self.chars[self.pos..].iter().take(3).collect();
        if next3 != "::=" {
            return Err(self.err("Expected ::="));
        }
        self.advance(3);
        self.skip_space(true);

        let body = self.parse_alternatives(false)?;
        self.skip_newline();

        let comment = std::mem::take(&mut self.curr_comment);
        Ok(Rule {
            name,
            body,
            comment,
            is_terminal: false,
            order: 0,
        })
    }

    fn parse_alternatives(&mut self, nested: bool) -> Result<Node, GbnfToLarkError> {
        let mut alts = vec![self.parse_sequence(nested)?];
        loop {
            self.skip_space(nested);
            if self.current() != Some('|') {
                break;
            }
            self.advance(1);
            self.skip_space(true);
            alts.push(self.parse_sequence(nested)?);
        }
        Ok(Node::Alternative(alts))
    }

    fn parse_sequence(&mut self, nested: bool) -> Result<Node, GbnfToLarkError> {
        let mut nodes: Vec<Node> = Vec::new();
        loop {
            match self.current() {
                None => break,
                Some('|') | Some(')') => break,
                Some(c) if !nested && (c == '\r' || c == '\n') => break,
                Some('"') => nodes.push(self.parse_literal()?),
                Some('[') => nodes.push(self.parse_char_class()?),
                Some('(') => nodes.push(self.parse_group(nested)?),
                Some('.') => {
                    nodes.push(Node::Regex(".".to_string()));
                    self.advance(1);
                }
                // GBNF token reference: <name> or <[42]>
                Some('<') => nodes.push(self.parse_special_token(false)?),
                // GBNF negated token reference: !<name>
                Some('!') if self.chars.get(self.pos + 1) == Some(&'<') => {
                    self.advance(1);
                    nodes.push(self.parse_special_token(true)?);
                }
                Some(c) if Self::is_word_char(c) => {
                    let name = self.parse_name()?;
                    nodes.push(Node::RuleRef(name));
                }
                _ => break,
            }
            self.skip_space(nested);
            self.parse_repetition(&mut nodes)?;
            self.skip_space(nested);
        }
        Ok(Node::Sequence(nodes))
    }

    fn parse_group(&mut self, nested: bool) -> Result<Node, GbnfToLarkError> {
        if self.current() != Some('(') {
            return Err(self.err("Expected '('"));
        }
        self.advance(1);
        self.skip_space(true);
        let node = self.parse_alternatives(true)?;
        if self.current() != Some(')') {
            return Err(self.err("Expected ')'"));
        }
        self.advance(1);
        self.skip_space(nested);
        Ok(node)
    }

    fn parse_repetition(&mut self, nodes: &mut Vec<Node>) -> Result<(), GbnfToLarkError> {
        if nodes.is_empty() {
            return Ok(());
        }
        let last_idx = nodes.len() - 1;
        match self.current() {
            Some('*') => {
                self.advance(1);
                let last = nodes.remove(last_idx);
                nodes.push(Node::Repetition {
                    node: Box::new(last),
                    min: 0,
                    max: None,
                });
            }
            Some('+') => {
                self.advance(1);
                let last = nodes.remove(last_idx);
                nodes.push(Node::Repetition {
                    node: Box::new(last),
                    min: 1,
                    max: None,
                });
            }
            Some('?') => {
                self.advance(1);
                let last = nodes.remove(last_idx);
                nodes.push(Node::Repetition {
                    node: Box::new(last),
                    min: 0,
                    max: Some(1),
                });
            }
            Some('{') => {
                self.advance(1);
                self.skip_space(true);
                let min = self.parse_int()?;
                self.skip_space(true);
                if self.current() == Some('}') {
                    self.advance(1);
                    let last = nodes.remove(last_idx);
                    nodes.push(Node::Repetition {
                        node: Box::new(last),
                        min,
                        max: Some(min),
                    });
                } else if self.current() == Some(',') {
                    self.advance(1);
                    self.skip_space(true);
                    let max = if self.current().is_some_and(|c| c.is_ascii_digit()) {
                        Some(self.parse_int()?)
                    } else {
                        None
                    };
                    self.skip_space(true);
                    if self.current() != Some('}') {
                        return Err(self.err("Expected '}'"));
                    }
                    self.advance(1);
                    let last = nodes.remove(last_idx);
                    nodes.push(Node::Repetition {
                        node: Box::new(last),
                        min,
                        max,
                    });
                } else {
                    return Err(self.err("Expected ',' or '}'"));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn parse_all(&mut self) -> Result<Vec<Rule>, GbnfToLarkError> {
        let mut rules = Vec::new();
        self.skip_space(true);
        while self.current().is_some() {
            rules.push(self.parse_rule()?);
            self.skip_space(true);
        }
        Ok(rules)
    }
}

// ===== Resolve =====

fn camel_to_snake(name: &str) -> String {
    let s = name.replace('-', "_");
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && c.is_uppercase() && chars[i - 1].is_lowercase() {
            result.push('_');
        }
        result.push(c);
    }
    result
}

fn to_lark_name(name: &str, is_terminal: bool) -> String {
    let snake = camel_to_snake(name);
    if is_terminal {
        snake.to_uppercase()
    } else {
        snake.to_lowercase()
    }
}

fn resolve(rules: &mut [Rule], entry_name: &str) -> Result<(), GbnfToLarkError> {
    // 1. Assign declaration order; simplify AST
    for (i, rule) in rules.iter_mut().enumerate() {
        rule.order = i;
        rule.body = rule.body.clone().simplify();
    }

    // 2. Validate all rule refs
    let known_names: HashSet<String> = rules.iter().map(|r| r.name.clone()).collect();
    for rule in rules.iter() {
        rule.body.validate_refs(&known_names)?;
    }

    // 3. Rename the entry rule to "start"; update all refs.
    // If entry_name is already "start", there's nothing to do at this step.
    if entry_name != "start" {
        let entry_idx = rules
            .iter()
            .position(|r| r.name == entry_name)
            .ok_or_else(|| {
                GbnfToLarkError::ResolutionError(format!("Entry rule '{}' not found", entry_name))
            })?;
        rules[entry_idx].name = "start".to_string();
        for rule in rules.iter_mut() {
            rule.body.rename_refs(entry_name, "start");
        }
    }

    // 4. Terminal detection fixpoint (skip "start")
    let mut terminal_names: HashSet<String> = HashSet::new();
    loop {
        let mut changed = false;
        for rule in rules.iter_mut() {
            if rule.name != "start" && !rule.is_terminal && rule.body.is_terminal(&terminal_names) {
                rule.is_terminal = true;
                terminal_names.insert(rule.name.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // 5. Compute final lark names; build rename map; apply to all rules and refs
    let rename_map: HashMap<String, String> = rules
        .iter()
        .map(|r| (r.name.clone(), to_lark_name(&r.name, r.is_terminal)))
        .collect();

    for rule in rules.iter_mut() {
        rule.name = rename_map[&rule.name].clone();
        rule.body.apply_rename_map(&rename_map);
    }

    Ok(())
}

// ===== Public functions =====

/// Convert a GBNF grammar string to Lark syntax, treating the rule literally
/// named `"root"` as the entry point (the llama.cpp GBNF convention).
///
/// If your grammar uses a different entry rule name, use
/// [`gbnf_to_lark_with_entry`] instead.
pub fn gbnf_to_lark(text: &str) -> Result<String, GbnfToLarkError> {
    gbnf_to_lark_with_entry(text, "root")
}

/// Convert a GBNF grammar string to Lark syntax, using `entry_name` as the
/// entry rule (renamed to `start` in the output).
pub fn gbnf_to_lark_with_entry(text: &str, entry_name: &str) -> Result<String, GbnfToLarkError> {
    let mut parser = Parser::new(text);
    let mut rules = parser.parse_all()?;
    resolve(&mut rules, entry_name)?;
    rules.sort_by_key(|r| r.order);

    let mut out = String::from("%llguidance {}\n\n");
    let mut prev_had_newline = true;

    for rule in &rules {
        let body_str = rule.body.to_lark_top();
        let rule_str = format!("{}{}: {}", rule.comment, rule.name, body_str);
        let has_nl = rule_str.contains('\n');

        if !prev_had_newline && has_nl {
            out.push('\n');
        }
        out.push_str(&rule_str);
        out.push('\n');
        prev_had_newline = has_nl;
        if prev_had_newline {
            out.push('\n');
        }
    }

    Ok(out)
}

/// Return true if the text is already in Lark syntax.
pub fn is_lark_syntax(text: &str) -> bool {
    text.lines().any(|line| {
        let t = line.trim_start();
        if t.starts_with("%llguidance") {
            return true;
        }
        let after_name = t.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_');
        if after_name.is_empty() || t == after_name {
            return false;
        }
        let rest = after_name.trim_start();
        rest.starts_with(':') && !rest.starts_with("::")
    })
}

/// Convert to Lark only if not already in Lark syntax; otherwise pass through unchanged.
pub fn any_to_lark(text: &str) -> Result<String, GbnfToLarkError> {
    if is_lark_syntax(text) {
        Ok(text.to_string())
    } else {
        gbnf_to_lark(text)
    }
}

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_literal() {
        let gbnf = r#"root ::= "hello""#;
        let lark = gbnf_to_lark(gbnf).unwrap();
        assert!(lark.starts_with("%llguidance {}"), "{}", lark);
        assert!(lark.contains("start:"), "{}", lark);
        assert!(lark.contains("\"hello\""), "{}", lark);
    }

    #[test]
    fn test_alternation() {
        let gbnf = r#"root ::= "yes" | "no""#;
        let lark = gbnf_to_lark(gbnf).unwrap();
        assert!(lark.contains("start:"), "{}", lark);
        assert!(lark.contains("\"yes\""), "{}", lark);
        assert!(lark.contains("\"no\""), "{}", lark);
    }

    #[test]
    fn test_terminal_char_class() {
        let gbnf = "digit ::= [0-9]\nroot ::= digit+";
        let lark = gbnf_to_lark(gbnf).unwrap();
        assert!(
            lark.contains("DIGIT:"),
            "Expected DIGIT terminal rule:\n{}",
            lark
        );
        assert!(
            lark.contains("DIGIT+"),
            "Expected DIGIT+ in start:\n{}",
            lark
        );
    }

    #[test]
    fn test_camel_case_normalization() {
        // jsonValue ::= "x"  →  body is a literal, so it's terminal  →  JSON_VALUE
        let gbnf = "jsonValue ::= \"x\"\nroot ::= jsonValue";
        let lark = gbnf_to_lark(gbnf).unwrap();
        assert!(
            lark.contains("JSON_VALUE:"),
            "Expected JSON_VALUE (camelCase+terminal):\n{}",
            lark
        );
    }

    #[test]
    fn test_camel_case_nonterminal_normalization() {
        // jsonArray wraps root (start), which is never terminal → json_array stays non-terminal
        let gbnf = "jsonArray ::= \"(\" root \")\"\nroot ::= [a-z]+";
        let lark = gbnf_to_lark(gbnf).unwrap();
        assert!(
            lark.contains("json_array:"),
            "Expected json_array (camelCase+non-terminal):\n{}",
            lark
        );
    }

    #[test]
    fn test_repetition_quantifiers() {
        let gbnf = "root ::= [a-z]* [0-9]+";
        let lark = gbnf_to_lark(gbnf).unwrap();
        assert!(lark.contains('*'), "{}", lark);
        assert!(lark.contains('+'), "{}", lark);
    }

    #[test]
    fn test_optional_quantifier() {
        let gbnf = "root ::= \"foo\"?";
        let lark = gbnf_to_lark(gbnf).unwrap();
        assert!(lark.contains('?'), "{}", lark);
    }

    #[test]
    fn test_exact_repetition() {
        let gbnf = "root ::= [0-9]{3}";
        let lark = gbnf_to_lark(gbnf).unwrap();
        assert!(lark.contains("{3}"), "{}", lark);
    }

    #[test]
    fn test_range_repetition() {
        let gbnf = "root ::= [0-9]{1,3}";
        let lark = gbnf_to_lark(gbnf).unwrap();
        assert!(lark.contains("{1,3}"), "{}", lark);
    }

    #[test]
    fn test_comment_preserved() {
        let gbnf = "# A comment\nroot ::= \"x\"";
        let lark = gbnf_to_lark(gbnf).unwrap();
        assert!(lark.contains("// A comment"), "{}", lark);
    }

    #[test]
    fn test_is_lark_syntax_llguidance_header() {
        assert!(is_lark_syntax("%llguidance {}\n\nstart: \"hello\""));
    }

    #[test]
    fn test_is_lark_syntax_start_rule() {
        assert!(is_lark_syntax("start: \"hello\""));
    }

    #[test]
    fn test_is_lark_syntax_false_for_gbnf() {
        assert!(!is_lark_syntax("root ::= \"hello\""));
    }

    #[test]
    fn test_any_to_lark_passthrough() {
        let lark = "%llguidance {}\n\nstart: \"hello\"\n";
        assert_eq!(any_to_lark(lark).unwrap(), lark);
    }

    #[test]
    fn test_any_to_lark_converts_gbnf() {
        let gbnf = "root ::= \"hello\"";
        let result = any_to_lark(gbnf).unwrap();
        assert!(result.starts_with("%llguidance"));
        assert!(result.contains("start:"));
    }

    #[test]
    fn test_missing_root_error() {
        let result = gbnf_to_lark("foo ::= \"bar\"");
        assert!(matches!(result, Err(GbnfToLarkError::ResolutionError(_))));
        assert!(result.unwrap_err().to_string().contains("root"));
    }

    #[test]
    fn test_custom_entry_rule_renamed_to_start() {
        // Grammar with a non-"root" entry rule, plus a "root" rule that should
        // remain untouched (this is the shape tool-calling handlers produce
        // when they wrap a JSON sub-grammar whose own entry is "root").
        let gbnf = "superroot ::= toolcall\ntoolcall ::= \"<\" root \">\"\nroot ::= [a-z]+";
        let lark = gbnf_to_lark_with_entry(gbnf, "superroot").unwrap();
        assert!(lark.contains("start:"), "expected start: rule:\n{}", lark);
        // The leftover "root" rule must not collide with the Lark entry.
        assert!(
            lark.contains("ROOT") || lark.contains("root"),
            "leftover root rule should survive:\n{}",
            lark
        );
        // start must reference what was originally "superroot"'s body.
        assert!(lark.contains("TOOLCALL") || lark.contains("toolcall"));
    }

    #[test]
    fn test_custom_entry_missing_errors() {
        let result = gbnf_to_lark_with_entry("foo ::= \"bar\"", "superroot");
        assert!(matches!(result, Err(GbnfToLarkError::ResolutionError(_))));
        assert!(result.unwrap_err().to_string().contains("superroot"));
    }

    #[test]
    fn test_undefined_ref_error() {
        let result = gbnf_to_lark("root ::= undefined-rule");
        assert!(matches!(result, Err(GbnfToLarkError::ResolutionError(_))));
    }

    #[test]
    fn test_unicode_escape() {
        let gbnf = "root ::= \"\\u0041\"";
        let lark = gbnf_to_lark(gbnf).unwrap();
        assert!(lark.contains("start:"), "{}", lark);
    }

    #[test]
    fn test_dot_becomes_regex() {
        let gbnf = "root ::= .+";
        let lark = gbnf_to_lark(gbnf).unwrap();
        assert!(lark.contains("/./"), "{}", lark);
    }

    #[test]
    fn test_multiline_alternatives_format() {
        let gbnf = "root ::= \"a\" | \"b\" | \"c\"";
        let lark = gbnf_to_lark(gbnf).unwrap();
        // Three alternatives at top level → multiline with " | " separator
        // (either inline or multiline depending on how simplify works, just check all present)
        assert!(lark.contains("\"a\""), "{}", lark);
        assert!(lark.contains("\"b\""), "{}", lark);
        assert!(lark.contains("\"c\""), "{}", lark);
    }

    #[test]
    fn test_json_grammar_roundtrip() {
        // A minimal JSON-like grammar to test a more complex case.
        // ws is all-terminal (char class) → WS
        // object's body is literals + WS (terminal) → OBJECT
        let gbnf = "root     ::= object\nobject   ::= \"{\" ws \"}\"\nws       ::= [ \\t\\n]*";
        let lark = gbnf_to_lark(gbnf).unwrap();
        assert!(lark.contains("start:"), "{}", lark);
        assert!(
            lark.contains("WS:"),
            "ws should become terminal WS:\n{}",
            lark
        );
        assert!(
            lark.contains("OBJECT:"),
            "object should become terminal OBJECT:\n{}",
            lark
        );
    }
}
