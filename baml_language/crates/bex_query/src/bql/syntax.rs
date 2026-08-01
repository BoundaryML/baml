use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::QueryError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Value {
    String(String),
    Integer(i64),
    Number(f64),
    Bool(bool),
    Null,
    Identifier(String),
    Human(String),
    List(Vec<Value>),
    Stage(Box<StageCall>),
    Param(String),
    Expr(Expression),
}

impl Value {
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value)
            | Self::Identifier(value)
            | Self::Human(value)
            | Self::Param(value) => Some(value),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Integer(value) => u64::try_from(*value).ok(),
            Self::Human(value) | Self::Identifier(value) => value.parse().ok(),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::Identifier(value) if value == "true" => Some(true),
            Self::Identifier(value) if value == "false" => Some(false),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Expression {
    pub field: String,
    pub op: CompareOp,
    pub value: Box<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Argument {
    pub name: Option<String>,
    pub value: Value,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StageCall {
    pub name: String,
    pub arguments: Vec<Argument>,
    pub span: Span,
}

impl StageCall {
    #[must_use]
    pub fn named(&self, name: &str) -> Option<&Value> {
        self.arguments
            .iter()
            .find(|argument| argument.name.as_deref() == Some(name))
            .map(|argument| &argument.value)
    }

    #[must_use]
    pub fn positional(&self, index: usize) -> Option<&Value> {
        self.arguments
            .iter()
            .filter(|argument| argument.name.is_none())
            .nth(index)
            .map(|argument| &argument.value)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pipeline {
    pub stages: Vec<StageCall>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Statement {
    pub name: Option<String>,
    pub pipeline: Pipeline,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Script {
    pub source: String,
    pub statements: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq)]
enum TokenKind {
    Identifier(String),
    String(String),
    Atom(String),
    Param(String),
    Pipe,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Equal,
    EqualEqual,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Semicolon,
    Eof,
}

#[derive(Clone, Debug, PartialEq)]
struct Token {
    kind: TokenKind,
    span: Span,
}

pub fn parse(source: &str) -> Result<Script, QueryError> {
    let tokens = lex(source)?;
    Parser {
        source,
        tokens,
        cursor: 0,
    }
    .script()
}

pub fn bind_params(
    script: &mut Script,
    params: &BTreeMap<String, String>,
) -> Result<(), QueryError> {
    fn bind_value(
        value: &mut Value,
        params: &BTreeMap<String, String>,
        source: &str,
    ) -> Result<(), QueryError> {
        match value {
            Value::Param(name) => {
                let replacement = params.get(name).ok_or_else(|| {
                    QueryError::bql(
                        "E_PARAM",
                        source,
                        0,
                        1,
                        format!("missing query parameter `${name}`"),
                    )
                })?;
                *value = parse_param_value(replacement);
            }
            Value::List(values) => {
                for value in values {
                    bind_value(value, params, source)?;
                }
            }
            Value::Stage(stage) => {
                for argument in &mut stage.arguments {
                    bind_value(&mut argument.value, params, source)?;
                }
            }
            Value::Expr(expression) => bind_value(&mut expression.value, params, source)?,
            _ => {}
        }
        Ok(())
    }

    for statement in &mut script.statements {
        for stage in &mut statement.pipeline.stages {
            for argument in &mut stage.arguments {
                bind_value(&mut argument.value, params, &script.source)?;
            }
        }
    }
    Ok(())
}

fn parse_param_value(value: &str) -> Value {
    match value {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        _ => value
            .parse::<i64>()
            .map_or_else(|_| Value::String(value.to_owned()), Value::Integer),
    }
}

fn lex(source: &str) -> Result<Vec<Token>, QueryError> {
    let bytes = source.as_bytes();
    let mut output = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        match bytes[cursor] {
            byte if byte.is_ascii_whitespace() => cursor += 1,
            b'#' => {
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor] != b'\n' {
                    cursor += 1;
                }
            }
            b'|' => push_simple(&mut output, TokenKind::Pipe, cursor, &mut cursor),
            b'(' => push_simple(&mut output, TokenKind::LParen, cursor, &mut cursor),
            b')' => push_simple(&mut output, TokenKind::RParen, cursor, &mut cursor),
            b'[' => push_simple(&mut output, TokenKind::LBracket, cursor, &mut cursor),
            b']' => push_simple(&mut output, TokenKind::RBracket, cursor, &mut cursor),
            b',' => push_simple(&mut output, TokenKind::Comma, cursor, &mut cursor),
            b';' => push_simple(&mut output, TokenKind::Semicolon, cursor, &mut cursor),
            b'=' if bytes.get(cursor + 1) == Some(&b'=') => {
                output.push(Token {
                    kind: TokenKind::EqualEqual,
                    span: Span {
                        start: cursor,
                        end: cursor + 2,
                    },
                });
                cursor += 2;
            }
            b'=' => push_simple(&mut output, TokenKind::Equal, cursor, &mut cursor),
            b'!' if bytes.get(cursor + 1) == Some(&b'=') => {
                output.push(Token {
                    kind: TokenKind::NotEqual,
                    span: Span {
                        start: cursor,
                        end: cursor + 2,
                    },
                });
                cursor += 2;
            }
            b'>' => lex_comparison(
                &mut output,
                bytes,
                &mut cursor,
                TokenKind::Greater,
                TokenKind::GreaterEqual,
            ),
            b'<' => lex_comparison(
                &mut output,
                bytes,
                &mut cursor,
                TokenKind::Less,
                TokenKind::LessEqual,
            ),
            b'"' | b'\'' => output.push(lex_string(source, &mut cursor)?),
            b'$' => output.push(lex_param(source, &mut cursor)?),
            byte if is_identifier_start(byte) => output.push(lex_identifier(source, &mut cursor)),
            byte if byte.is_ascii_digit() || byte == b'-' => {
                output.push(lex_atom(source, &mut cursor))
            }
            _ => {
                return Err(QueryError::bql(
                    "E_LEX",
                    source,
                    cursor,
                    cursor + 1,
                    format!(
                        "unexpected character `{}`",
                        source[cursor..].chars().next().unwrap()
                    ),
                ));
            }
        }
    }
    output.push(Token {
        kind: TokenKind::Eof,
        span: Span {
            start: source.len(),
            end: source.len(),
        },
    });
    Ok(output)
}

fn push_simple(output: &mut Vec<Token>, kind: TokenKind, start: usize, cursor: &mut usize) {
    *cursor += 1;
    output.push(Token {
        kind,
        span: Span {
            start,
            end: *cursor,
        },
    });
}

fn lex_comparison(
    output: &mut Vec<Token>,
    bytes: &[u8],
    cursor: &mut usize,
    simple: TokenKind,
    equal: TokenKind,
) {
    let start = *cursor;
    *cursor += 1;
    let kind = if bytes.get(*cursor) == Some(&b'=') {
        *cursor += 1;
        equal
    } else {
        simple
    };
    output.push(Token {
        kind,
        span: Span {
            start,
            end: *cursor,
        },
    });
}

fn lex_string(source: &str, cursor: &mut usize) -> Result<Token, QueryError> {
    let bytes = source.as_bytes();
    let start = *cursor;
    let quote = bytes[*cursor];
    *cursor += 1;
    let mut value = String::new();
    while *cursor < bytes.len() {
        let byte = bytes[*cursor];
        if !byte.is_ascii() {
            let character = source[*cursor..]
                .chars()
                .next()
                .expect("cursor is within source");
            value.push(character);
            *cursor += character.len_utf8();
            continue;
        }
        *cursor += 1;
        if byte == quote {
            return Ok(Token {
                kind: TokenKind::String(value),
                span: Span {
                    start,
                    end: *cursor,
                },
            });
        }
        if byte == b'\\' {
            let escaped = *bytes.get(*cursor).ok_or_else(|| {
                QueryError::bql("E_STRING", source, start, *cursor, "unterminated escape")
            })?;
            *cursor += 1;
            value.push(match escaped {
                b'n' => '\n',
                b'r' => '\r',
                b't' => '\t',
                b'\\' => '\\',
                b'"' => '"',
                b'\'' => '\'',
                _ => {
                    return Err(QueryError::bql(
                        "E_STRING",
                        source,
                        cursor.saturating_sub(2),
                        *cursor,
                        "unsupported string escape",
                    ));
                }
            });
        } else {
            value.push(char::from(byte));
        }
    }
    Err(QueryError::bql(
        "E_STRING",
        source,
        start,
        source.len(),
        "unterminated string literal",
    ))
}

fn lex_param(source: &str, cursor: &mut usize) -> Result<Token, QueryError> {
    let start = *cursor;
    *cursor += 1;
    let name_start = *cursor;
    while source
        .as_bytes()
        .get(*cursor)
        .is_some_and(|byte| is_identifier_continue(*byte))
    {
        *cursor += 1;
    }
    if *cursor == name_start {
        return Err(QueryError::bql(
            "E_PARAM",
            source,
            start,
            *cursor,
            "expected a parameter name after `$`",
        ));
    }
    Ok(Token {
        kind: TokenKind::Param(source[name_start..*cursor].to_owned()),
        span: Span {
            start,
            end: *cursor,
        },
    })
}

fn lex_identifier(source: &str, cursor: &mut usize) -> Token {
    let start = *cursor;
    *cursor += 1;
    while source
        .as_bytes()
        .get(*cursor)
        .is_some_and(|byte| is_identifier_continue(*byte))
    {
        *cursor += 1;
    }
    Token {
        kind: TokenKind::Identifier(source[start..*cursor].to_owned()),
        span: Span {
            start,
            end: *cursor,
        },
    }
}

fn lex_atom(source: &str, cursor: &mut usize) -> Token {
    let start = *cursor;
    *cursor += 1;
    while source.as_bytes().get(*cursor).is_some_and(|byte| {
        byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b':' | b'%' | b'-')
    }) {
        *cursor += 1;
    }
    Token {
        kind: TokenKind::Atom(source[start..*cursor].to_owned()),
        span: Span {
            start,
            end: *cursor,
        },
    }
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-' | b'*')
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    cursor: usize,
}

impl Parser<'_> {
    fn script(mut self) -> Result<Script, QueryError> {
        let mut statements = Vec::new();
        while !self.at(|kind| matches!(kind, TokenKind::Eof)) {
            let name = if let (
                Some(Token {
                    kind: TokenKind::Identifier(name),
                    ..
                }),
                Some(Token {
                    kind: TokenKind::Equal,
                    ..
                }),
            ) = (
                self.tokens.get(self.cursor),
                self.tokens.get(self.cursor + 1),
            ) {
                let name = name.clone();
                self.cursor += 2;
                Some(name)
            } else {
                None
            };
            statements.push(Statement {
                name,
                pipeline: self.pipeline()?,
            });
            if self.at(|kind| matches!(kind, TokenKind::Semicolon)) {
                self.cursor += 1;
            } else if !self.at(|kind| matches!(kind, TokenKind::Eof)) {
                return Err(self.error_here("E_PARSE", "expected `;` between BQL statements"));
            }
        }
        if statements.is_empty() {
            return Err(QueryError::bql(
                "E_PARSE",
                self.source,
                0,
                1,
                "BQL query is empty",
            ));
        }
        Ok(Script {
            source: self.source.to_owned(),
            statements,
        })
    }

    fn pipeline(&mut self) -> Result<Pipeline, QueryError> {
        let start = self.peek().span.start;
        let mut stages = vec![self.stage()?];
        while self.at(|kind| matches!(kind, TokenKind::Pipe)) {
            self.cursor += 1;
            stages.push(self.stage()?);
        }
        let end = stages.last().map_or(start, |stage| stage.span.end);
        Ok(Pipeline {
            stages,
            span: Span { start, end },
        })
    }

    fn stage(&mut self) -> Result<StageCall, QueryError> {
        let token = self.next().clone();
        let TokenKind::Identifier(name) = token.kind else {
            return Err(QueryError::bql(
                "E_PARSE",
                self.source,
                token.span.start,
                token.span.end,
                "expected a stage name",
            ));
        };
        let mut arguments = Vec::new();
        let mut end = token.span.end;
        if self.at(|kind| matches!(kind, TokenKind::LParen)) {
            self.cursor += 1;
            if !self.at(|kind| matches!(kind, TokenKind::RParen)) {
                loop {
                    arguments.push(self.argument()?);
                    if self.at(|kind| matches!(kind, TokenKind::Comma)) {
                        self.cursor += 1;
                        continue;
                    }
                    break;
                }
            }
            let close = self.expect(
                |kind| matches!(kind, TokenKind::RParen),
                "expected `)` after stage arguments",
            )?;
            end = close.span.end;
        }
        Ok(StageCall {
            name,
            arguments,
            span: Span {
                start: token.span.start,
                end,
            },
        })
    }

    fn argument(&mut self) -> Result<Argument, QueryError> {
        let start = self.peek().span.start;
        let name = if let (
            Some(Token {
                kind: TokenKind::Identifier(name),
                ..
            }),
            Some(Token {
                kind: TokenKind::Equal,
                ..
            }),
        ) = (
            self.tokens.get(self.cursor),
            self.tokens.get(self.cursor + 1),
        ) {
            let name = name.clone();
            self.cursor += 2;
            Some(name)
        } else {
            None
        };
        let value = self.value()?;
        if name.is_none() && self.is_comparison() {
            let Value::Identifier(field) = value else {
                return Err(
                    self.error_here("E_PARSE", "left side of a comparison must be a field name")
                );
            };
            let op = self.comparison()?;
            let rhs = self.value()?;
            let end = self.previous().span.end;
            return Ok(Argument {
                name,
                value: Value::Expr(Expression {
                    field,
                    op,
                    value: Box::new(rhs),
                }),
                span: Span { start, end },
            });
        }
        let end = self.previous().span.end;
        Ok(Argument {
            name,
            value,
            span: Span { start, end },
        })
    }

    fn value(&mut self) -> Result<Value, QueryError> {
        let token = self.next().clone();
        match token.kind {
            TokenKind::String(value) => Ok(Value::String(value)),
            TokenKind::Param(value) => Ok(Value::Param(value)),
            TokenKind::Atom(value) => {
                if let Ok(integer) = value.parse() {
                    Ok(Value::Integer(integer))
                } else if let Ok(number) = value.parse() {
                    Ok(Value::Number(number))
                } else {
                    Ok(Value::Human(value))
                }
            }
            TokenKind::Identifier(value) => {
                if self.at(|kind| matches!(kind, TokenKind::LParen)) {
                    self.cursor = self.cursor.saturating_sub(1);
                    return self.stage().map(|stage| Value::Stage(Box::new(stage)));
                }
                Ok(match value.as_str() {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    "null" => Value::Null,
                    _ => Value::Identifier(value),
                })
            }
            TokenKind::LBracket => {
                let mut values = Vec::new();
                if !self.at(|kind| matches!(kind, TokenKind::RBracket)) {
                    loop {
                        values.push(self.value()?);
                        if self.at(|kind| matches!(kind, TokenKind::Comma)) {
                            self.cursor += 1;
                        } else {
                            break;
                        }
                    }
                }
                self.expect(
                    |kind| matches!(kind, TokenKind::RBracket),
                    "expected `]` after list",
                )?;
                Ok(Value::List(values))
            }
            _ => Err(QueryError::bql(
                "E_PARSE",
                self.source,
                token.span.start,
                token.span.end,
                "expected a BQL literal, list, parameter, or nested stage",
            )),
        }
    }

    fn is_comparison(&self) -> bool {
        self.at(|kind| {
            matches!(
                kind,
                TokenKind::EqualEqual
                    | TokenKind::NotEqual
                    | TokenKind::Greater
                    | TokenKind::GreaterEqual
                    | TokenKind::Less
                    | TokenKind::LessEqual
            )
        })
    }

    fn comparison(&mut self) -> Result<CompareOp, QueryError> {
        let token = self.next();
        match token.kind {
            TokenKind::EqualEqual => Ok(CompareOp::Eq),
            TokenKind::NotEqual => Ok(CompareOp::Ne),
            TokenKind::Greater => Ok(CompareOp::Gt),
            TokenKind::GreaterEqual => Ok(CompareOp::Ge),
            TokenKind::Less => Ok(CompareOp::Lt),
            TokenKind::LessEqual => Ok(CompareOp::Le),
            _ => Err(self.error_here("E_PARSE", "expected comparison operator")),
        }
    }

    fn expect(
        &mut self,
        predicate: impl FnOnce(&TokenKind) -> bool,
        message: &str,
    ) -> Result<Token, QueryError> {
        if predicate(&self.peek().kind) {
            Ok(self.next().clone())
        } else {
            Err(self.error_here("E_PARSE", message))
        }
    }

    fn at(&self, predicate: impl FnOnce(&TokenKind) -> bool) -> bool {
        predicate(&self.peek().kind)
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.cursor]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.cursor.saturating_sub(1)]
    }

    fn next(&mut self) -> &Token {
        let index = self.cursor;
        self.cursor = (self.cursor + 1).min(self.tokens.len().saturating_sub(1));
        &self.tokens[index]
    }

    fn error_here(&self, code: &'static str, message: &str) -> QueryError {
        QueryError::bql(
            code,
            self.source,
            self.peek().span.start,
            self.peek().span.end,
            message,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pipeline_nested_sources_lists_expressions_and_params() {
        let mut script = parse(
            r#"out = diff(runs(rev=$old), runs(rev=$new), align=fqn)
                | compare(metrics=[calls, errors], match_io=false)
                | where(errors > 1);"#,
        )
        .unwrap();
        bind_params(
            &mut script,
            &BTreeMap::from([
                ("old".to_owned(), "rev-a".to_owned()),
                ("new".to_owned(), "rev-b".to_owned()),
            ]),
        )
        .unwrap();
        assert_eq!(script.statements[0].name.as_deref(), Some("out"));
        assert_eq!(script.statements[0].pipeline.stages.len(), 3);
        assert!(matches!(
            script.statements[0].pipeline.stages[2].positional(0),
            Some(Value::Expr(_))
        ));
    }

    #[test]
    fn diagnostics_have_a_caret_location() {
        let error = parse("runs() | top(10, by=calls").unwrap_err();
        let diagnostic = error.diagnostic().unwrap();
        assert_eq!(diagnostic.code, "E_PARSE");
        assert_eq!(diagnostic.line, 1);
        assert!(error.to_string().contains('^'));
    }
}
