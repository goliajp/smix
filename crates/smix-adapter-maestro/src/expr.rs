//! Minimal yaml expression engine (adapter-internal).
//!
//! Implements a JS-like literal closure evaluator for Maestro-style
//! `${...}` interpolation. Rather than embedding a JS engine
//! (`evalexpr` lacks `.contains()`, `rhai` is not JS, `boa_engine` /
//! `deno_core` are too large), the engine is a small hand-written AST +
//! recursive-descent parser + tree-walker evaluator.
//!
//! # Grammar (precedence low → high)
//!
//! ```text
//! or       = and  ("||" and)*
//! and      = eq   ("&&" eq)*
//! eq       = unary (("==" | "!=") unary)*
//! unary    = "!" unary | postfix
//! postfix  = primary (".contains" "(" expr ")")*
//! primary  = literal | varRef | "(" expr ")"
//! literal  = string | number | "true" | "false" | "null"
//! varRef   = "output" ("." Ident | "[" string "]")+
//! ```
//!
//! Arithmetic, ternary, function definitions, object literals, string
//! concatenation, and chained methods are **not** supported and raise
//! [`ExprError::UnsupportedPattern`] rather than silently no-op.

use std::collections::BTreeMap;
use std::fmt;

/// Runtime value within the expr engine. Minimal subset (Null / Bool /
/// Number / String) — the full `serde_json` value is not pulled in; a
/// `From<&serde_json::Value>` blanket conversion is sufficient since
/// the output store is `BTreeMap<String, Value>`.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// JSON null / undefined variable fallback.
    Null,
    /// `true` / `false` literal.
    Bool(bool),
    /// f64 number literal.
    Number(f64),
    /// quoted string literal or output store string value.
    String(String),
}

impl Value {
    /// JS-truthy semantics: false / null / 0 / "" → falsy; else truthy.
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0 && !n.is_nan(),
            Value::String(s) => !s.is_empty(),
        }
    }

    /// Stringify for template substitution (`${output.x}` → "value").
    pub fn to_template_string(&self) -> String {
        match self {
            Value::Null => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => {
                if n.fract() == 0.0 && n.is_finite() {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            Value::String(s) => s.clone(),
        }
    }
}

/// Evaluation context: holds the output store (yaml flow-level
/// globals) + env store (`--env KEY=VAL` from the CLI + the inherited
/// process env). Output starts empty; `as: name` writes populate it.
///
/// Variable lookup priority: bare `${NAME}` first checks `output` (for
/// flow-captured aliases like `${output.email}`), then `env` (for
/// external-consumer keys such as `${E2E_EMAIL}` / `${IMAP_PASSWORD}`).
#[derive(Clone, Debug, Default)]
pub struct Context {
    pub output: BTreeMap<String, Value>,
    pub env: BTreeMap<String, String>,
}

/// Errors surfaced from the engine. AI-readable: snippet + hint feed
/// into the runtime DriverError message + suggestions.
#[derive(Clone, Debug, PartialEq)]
pub enum ExprError {
    EmptyExpression,
    UnexpectedToken { snippet: String },
    UndefinedVariable { path: String },
    UnsupportedPattern { snippet: String, hint: String },
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExprError::EmptyExpression => write!(f, "empty expression"),
            ExprError::UnexpectedToken { snippet } => {
                write!(f, "unexpected token: {snippet}")
            }
            ExprError::UndefinedVariable { path } => {
                write!(f, "undefined variable: {path}")
            }
            ExprError::UnsupportedPattern { snippet, hint } => {
                write!(f, "unsupported expression pattern '{snippet}': {hint}")
            }
        }
    }
}

impl std::error::Error for ExprError {}

const UNSUPPORTED_HINT: &str = "yaml expression engine supports == != && || ! () output.x .contains(); arithmetic / ternary / function / string-concat are not supported. Rewrite using literals or split into steps.";

// --------------------------------------------------------------------
// Tokenizer
// --------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Ident(String),
    StringLit(String),
    NumberLit(f64),
    True,
    False,
    Null,
    EqEq,
    NotEq,
    AndAnd,
    OrOr,
    Bang,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Dot,
    Comma,
}

fn tokenize(src: &str) -> Result<Vec<Token>, ExprError> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // unsupported single-char arithmetic / ternary tokens.
        if matches!(b, b'+' | b'-' | b'*' | b'/' | b'%' | b'?' | b':') {
            // unary minus on numbers is not currently allowed.
            return Err(ExprError::UnsupportedPattern {
                snippet: (b as char).to_string(),
                hint: UNSUPPORTED_HINT.to_string(),
            });
        }
        match b {
            b'(' => {
                out.push(Token::LParen);
                i += 1;
            }
            b')' => {
                out.push(Token::RParen);
                i += 1;
            }
            b'[' => {
                out.push(Token::LBracket);
                i += 1;
            }
            b']' => {
                out.push(Token::RBracket);
                i += 1;
            }
            b'.' => {
                out.push(Token::Dot);
                i += 1;
            }
            b',' => {
                out.push(Token::Comma);
                i += 1;
            }
            b'!' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    out.push(Token::NotEq);
                    i += 2;
                } else {
                    out.push(Token::Bang);
                    i += 1;
                }
            }
            b'=' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    out.push(Token::EqEq);
                    i += 2;
                } else {
                    return Err(ExprError::UnsupportedPattern {
                        snippet: "=".to_string(),
                        hint: "single `=` (assignment) not supported; use `==` for comparison"
                            .to_string(),
                    });
                }
            }
            b'&' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'&' {
                    out.push(Token::AndAnd);
                    i += 2;
                } else {
                    return Err(ExprError::UnsupportedPattern {
                        snippet: "&".to_string(),
                        hint: "single `&` (bitwise) not supported; use `&&` for logical AND"
                            .to_string(),
                    });
                }
            }
            b'|' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'|' {
                    out.push(Token::OrOr);
                    i += 2;
                } else {
                    return Err(ExprError::UnsupportedPattern {
                        snippet: "|".to_string(),
                        hint: "single `|` (bitwise) not supported; use `||` for logical OR"
                            .to_string(),
                    });
                }
            }
            b'"' | b'\'' => {
                // string literal — supports backslash escape (\\ \" \n \t).
                let quote = b;
                i += 1;
                let mut s = String::new();
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        let esc = bytes[i + 1];
                        match esc {
                            b'\\' => s.push('\\'),
                            b'"' => s.push('"'),
                            b'\'' => s.push('\''),
                            b'n' => s.push('\n'),
                            b't' => s.push('\t'),
                            b'r' => s.push('\r'),
                            _ => s.push(esc as char),
                        }
                        i += 2;
                    } else {
                        s.push(bytes[i] as char);
                        i += 1;
                    }
                }
                if i >= bytes.len() {
                    return Err(ExprError::UnexpectedToken {
                        snippet: "unterminated string literal".to_string(),
                    });
                }
                i += 1; // skip closing quote
                out.push(Token::StringLit(s));
            }
            d if d.is_ascii_digit() => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                    i += 1;
                }
                let slice = std::str::from_utf8(&bytes[start..i]).map_err(|_| {
                    ExprError::UnexpectedToken {
                        snippet: "invalid number".to_string(),
                    }
                })?;
                let n: f64 = slice.parse().map_err(|_| ExprError::UnexpectedToken {
                    snippet: slice.to_string(),
                })?;
                out.push(Token::NumberLit(n));
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let ident = std::str::from_utf8(&bytes[start..i]).unwrap().to_string();
                let tok = match ident.as_str() {
                    "true" => Token::True,
                    "false" => Token::False,
                    "null" => Token::Null,
                    _ => Token::Ident(ident),
                };
                out.push(tok);
            }
            _ => {
                return Err(ExprError::UnexpectedToken {
                    snippet: (b as char).to_string(),
                });
            }
        }
    }
    Ok(out)
}

// --------------------------------------------------------------------
// AST + Parser
// --------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Expr {
    Literal(Value),
    /// `output` then chain of `.field` / `["key"]` accessors (paths flat).
    OutputAccess(Vec<String>),
    /// Bare `${NAME}` — resolved from `Context.env`.
    EnvAccess(String),
    Not(Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    NotEq(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    /// `<expr>.contains(<expr>)`
    Contains(Box<Expr>, Box<Expr>),
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }
    fn advance(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn parse_or(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Some(Token::OrOr)) {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_eq()?;
        while matches!(self.peek(), Some(Token::AndAnd)) {
            self.advance();
            let right = self.parse_eq()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_eq(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_unary()?;
        loop {
            match self.peek() {
                Some(Token::EqEq) => {
                    self.advance();
                    let right = self.parse_unary()?;
                    left = Expr::Eq(Box::new(left), Box::new(right));
                }
                Some(Token::NotEq) => {
                    self.advance();
                    let right = self.parse_unary()?;
                    left = Expr::NotEq(Box::new(left), Box::new(right));
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ExprError> {
        if matches!(self.peek(), Some(Token::Bang)) {
            self.advance();
            let inner = self.parse_unary()?;
            Ok(Expr::Not(Box::new(inner)))
        } else {
            self.parse_postfix()
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_primary()?;
        // chain `.contains(<expr>)` postfix
        while matches!(self.peek(), Some(Token::Dot)) {
            self.advance();
            // expect Ident("contains") then "(" expr ")"
            let m = match self.advance() {
                Some(Token::Ident(name)) => name,
                other => {
                    return Err(ExprError::UnexpectedToken {
                        snippet: format!("{other:?}"),
                    });
                }
            };
            if m != "contains" {
                return Err(ExprError::UnsupportedPattern {
                    snippet: format!(".{m}(...)"),
                    hint: UNSUPPORTED_HINT.to_string(),
                });
            }
            if !matches!(self.advance(), Some(Token::LParen)) {
                return Err(ExprError::UnexpectedToken {
                    snippet: "expected '(' after .contains".to_string(),
                });
            }
            let arg = self.parse_or()?;
            if !matches!(self.advance(), Some(Token::RParen)) {
                return Err(ExprError::UnexpectedToken {
                    snippet: "expected ')' closing .contains".to_string(),
                });
            }
            left = Expr::Contains(Box::new(left), Box::new(arg));
        }
        Ok(left)
    }

    fn parse_primary(&mut self) -> Result<Expr, ExprError> {
        let tok = self.advance().ok_or(ExprError::EmptyExpression)?;
        match tok {
            Token::True => Ok(Expr::Literal(Value::Bool(true))),
            Token::False => Ok(Expr::Literal(Value::Bool(false))),
            Token::Null => Ok(Expr::Literal(Value::Null)),
            Token::StringLit(s) => Ok(Expr::Literal(Value::String(s))),
            Token::NumberLit(n) => Ok(Expr::Literal(Value::Number(n))),
            Token::LParen => {
                let inner = self.parse_or()?;
                if !matches!(self.advance(), Some(Token::RParen)) {
                    return Err(ExprError::UnexpectedToken {
                        snippet: "expected ')'".to_string(),
                    });
                }
                Ok(inner)
            }
            Token::Ident(name) => {
                if name == "output" {
                    // The output store is a flat map<String, Value>
                    // (no nested objects). Take the first `.<ident>` or
                    // `["key"]` as the key; further `.<...>` chains go
                    // to postfix (.contains) or are rejected at eval
                    // time (nested path).
                    let key = match self.peek() {
                        Some(Token::Dot) => {
                            self.advance();
                            match self.advance() {
                                Some(Token::Ident(field)) => field,
                                other => {
                                    return Err(ExprError::UnexpectedToken {
                                        snippet: format!("{other:?}"),
                                    });
                                }
                            }
                        }
                        Some(Token::LBracket) => {
                            self.advance();
                            let k = match self.advance() {
                                Some(Token::StringLit(s)) => s,
                                other => {
                                    return Err(ExprError::UnexpectedToken {
                                        snippet: format!("{other:?}"),
                                    });
                                }
                            };
                            if !matches!(self.advance(), Some(Token::RBracket)) {
                                return Err(ExprError::UnexpectedToken {
                                    snippet: "expected ']'".to_string(),
                                });
                            }
                            k
                        }
                        _ => {
                            return Err(ExprError::UnsupportedPattern {
                                snippet: "output".to_string(),
                                hint: "use `output.<field>` or `output[\"key\"]`".to_string(),
                            });
                        }
                    };
                    Ok(Expr::OutputAccess(vec![key]))
                } else {
                    // Bare `${NAME}` binds to Context.env — CLI
                    // `--env NAME=VAL` flags + the inherited process
                    // env. Nested access (`${FOO.bar}`) is not
                    // supported for env vars.
                    Ok(Expr::EnvAccess(name))
                }
            }
            other => Err(ExprError::UnexpectedToken {
                snippet: format!("{other:?}"),
            }),
        }
    }
}

// --------------------------------------------------------------------
// Evaluator
// --------------------------------------------------------------------

fn eval(expr: &Expr, ctx: &Context) -> Result<Value, ExprError> {
    match expr {
        Expr::Literal(v) => Ok(v.clone()),
        Expr::OutputAccess(path) => {
            // The output store is a flat map<String, Value>; a single
            // key indexes `ctx.output`. `path.len() > 1` no longer
            // arises from the parser (single-key lookup), but keep the
            // explicit-unsupported guard for internal consistency.
            if path.len() != 1 {
                return Err(ExprError::UnsupportedPattern {
                    snippet: format!("output.{}", path.join(".")),
                    hint: "nested output access (output.x.y) is not supported; flatten the key"
                        .to_string(),
                });
            }
            let key = &path[0];
            ctx.output
                .get(key)
                .cloned()
                .ok_or_else(|| ExprError::UndefinedVariable {
                    path: format!("output.{key}"),
                })
        }
        Expr::EnvAccess(name) => {
            // Bare `${NAME}` → env lookup. Missing key is an error
            // (matches Maestro `-e KEY=VAL` semantics where unset is
            // an author bug, not a silent empty string).
            ctx.env
                .get(name)
                .cloned()
                .map(Value::String)
                .ok_or_else(|| ExprError::UndefinedVariable { path: name.clone() })
        }
        Expr::Not(inner) => {
            let v = eval(inner, ctx)?;
            Ok(Value::Bool(!v.is_truthy()))
        }
        Expr::Eq(l, r) => {
            let lv = eval(l, ctx)?;
            let rv = eval(r, ctx)?;
            Ok(Value::Bool(values_equal(&lv, &rv)))
        }
        Expr::NotEq(l, r) => {
            let lv = eval(l, ctx)?;
            let rv = eval(r, ctx)?;
            Ok(Value::Bool(!values_equal(&lv, &rv)))
        }
        Expr::And(l, r) => {
            // short-circuit
            let lv = eval(l, ctx)?;
            if !lv.is_truthy() {
                return Ok(lv);
            }
            eval(r, ctx)
        }
        Expr::Or(l, r) => {
            let lv = eval(l, ctx)?;
            if lv.is_truthy() {
                return Ok(lv);
            }
            eval(r, ctx)
        }
        Expr::Contains(haystack_expr, needle_expr) => {
            let haystack = eval(haystack_expr, ctx)?;
            let needle = eval(needle_expr, ctx)?;
            match (&haystack, &needle) {
                (Value::String(h), Value::String(n)) => Ok(Value::Bool(h.contains(n.as_str()))),
                _ => Err(ExprError::UnsupportedPattern {
                    snippet: format!("{haystack:?}.contains({needle:?})"),
                    hint: ".contains() only supports string.contains(string)".to_string(),
                }),
            }
        }
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => x == y,
        (Value::String(x), Value::String(y)) => x == y,
        // mixed types compare via stringification (loose-equal lite)
        _ => false,
    }
}

// --------------------------------------------------------------------
// Public entry
// --------------------------------------------------------------------

/// Parse and evaluate `src` against `ctx`. One-shot entry — caller does
/// not see the AST.
pub fn parse_and_eval(src: &str, ctx: &Context) -> Result<Value, ExprError> {
    let trimmed = src.trim();
    if trimmed.is_empty() {
        return Err(ExprError::EmptyExpression);
    }
    let tokens = tokenize(trimmed)?;
    if tokens.is_empty() {
        return Err(ExprError::EmptyExpression);
    }
    let mut parser = Parser { tokens, pos: 0 };
    let expr = parser.parse_or()?;
    // tokens left unconsumed → unexpected trailing tokens.
    if parser.pos < parser.tokens.len() {
        return Err(ExprError::UnexpectedToken {
            snippet: format!("{:?}", parser.tokens[parser.pos]),
        });
    }
    eval(&expr, ctx)
}

// --------------------------------------------------------------------
// Unit tests (in-module; the expr engine is pure Rust — no I/O, no async).
// --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with(entries: &[(&str, Value)]) -> Context {
        let mut output = BTreeMap::new();
        for (k, v) in entries {
            output.insert((*k).to_string(), v.clone());
        }
        Context {
            output,
            env: BTreeMap::new(),
        }
    }

    #[test]
    fn literal_true_is_truthy() {
        let v = parse_and_eval("true", &Context::default()).unwrap();
        assert_eq!(v, Value::Bool(true));
        assert!(v.is_truthy());
    }

    #[test]
    fn literal_string_eq() {
        let v = parse_and_eval(r#""hello" == "hello""#, &Context::default()).unwrap();
        assert_eq!(v, Value::Bool(true));
    }

    #[test]
    fn output_var_read() {
        let ctx = ctx_with(&[("foo", Value::String("bar".to_string()))]);
        let v = parse_and_eval("output.foo == \"bar\"", &ctx).unwrap();
        assert_eq!(v, Value::Bool(true));
    }

    #[test]
    fn output_bracket_access() {
        let ctx = ctx_with(&[("k1", Value::Number(42.0))]);
        let v = parse_and_eval("output[\"k1\"] == 42", &ctx).unwrap();
        assert_eq!(v, Value::Bool(true));
    }

    #[test]
    fn and_or_logic() {
        let ctx = ctx_with(&[("a", Value::Bool(true)), ("b", Value::Bool(false))]);
        assert_eq!(
            parse_and_eval("output.a && !output.b", &ctx).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            parse_and_eval("output.a || output.b", &ctx).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            parse_and_eval("!output.a && output.b", &ctx).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn string_contains() {
        let ctx = ctx_with(&[("title", Value::String("Counting Areas".to_string()))]);
        let v = parse_and_eval(r#"output.title.contains("Counting")"#, &ctx).unwrap();
        assert_eq!(v, Value::Bool(true));
        let v2 = parse_and_eval(r#"output.title.contains("Crowd")"#, &ctx).unwrap();
        assert_eq!(v2, Value::Bool(false));
    }

    #[test]
    fn undefined_variable_errors() {
        let err = parse_and_eval("output.missing", &Context::default()).unwrap_err();
        assert!(matches!(err, ExprError::UndefinedVariable { path } if path == "output.missing"));
    }

    #[test]
    fn arithmetic_unsupported() {
        let err = parse_and_eval("1 + 2", &Context::default()).unwrap_err();
        assert!(matches!(err, ExprError::UnsupportedPattern { .. }));
    }

    #[test]
    fn ternary_unsupported() {
        let err = parse_and_eval("true ? 1 : 2", &Context::default()).unwrap_err();
        assert!(matches!(err, ExprError::UnsupportedPattern { .. }));
    }

    #[test]
    fn paren_precedence() {
        let v = parse_and_eval("(true || false) && false", &Context::default()).unwrap();
        assert_eq!(v, Value::Bool(false));
    }
}
