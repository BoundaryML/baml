//! Parser implementation.
//!
//! Implements a recursive descent parser with error recovery.

use baml_lexer::{Token, TokenKind};
use baml_syntax::SyntaxKind;
use rowan::{GreenNode, GreenNodeBuilder};
use text_size::TextRange;

use crate::ParseError;

/// Map lexer token kinds to syntax kinds.
fn token_kind_to_syntax_kind(kind: TokenKind) -> SyntaxKind {
    match kind {
        // Literals
        TokenKind::Word => SyntaxKind::WORD,
        TokenKind::Quote => SyntaxKind::QUOTE,
        TokenKind::Hash => SyntaxKind::HASH,
        TokenKind::Integer => SyntaxKind::INTEGER,
        TokenKind::Float => SyntaxKind::FLOAT,

        // Brackets
        TokenKind::LBrace => SyntaxKind::L_BRACE,
        TokenKind::RBrace => SyntaxKind::R_BRACE,
        TokenKind::LParen => SyntaxKind::L_PAREN,
        TokenKind::RParen => SyntaxKind::R_PAREN,
        TokenKind::LBracket => SyntaxKind::L_BRACKET,
        TokenKind::RBracket => SyntaxKind::R_BRACKET,

        // Punctuation
        TokenKind::Colon => SyntaxKind::COLON,
        TokenKind::DoubleColon => SyntaxKind::DOUBLE_COLON,
        TokenKind::Comma => SyntaxKind::COMMA,
        TokenKind::Semicolon => SyntaxKind::SEMICOLON,
        TokenKind::Dot => SyntaxKind::DOT,

        // Special operators
        TokenKind::Arrow => SyntaxKind::ARROW,
        TokenKind::At => SyntaxKind::AT,
        TokenKind::AtAt => SyntaxKind::AT_AT,
        TokenKind::Pipe => SyntaxKind::PIPE,
        TokenKind::Question => SyntaxKind::QUESTION,

        // Assignment operators
        TokenKind::Equals => SyntaxKind::EQUALS,
        TokenKind::PlusEquals => SyntaxKind::PLUS_EQUALS,
        TokenKind::MinusEquals => SyntaxKind::MINUS_EQUALS,
        TokenKind::StarEquals => SyntaxKind::STAR_EQUALS,
        TokenKind::SlashEquals => SyntaxKind::SLASH_EQUALS,
        TokenKind::PercentEquals => SyntaxKind::PERCENT_EQUALS,
        TokenKind::AndEquals => SyntaxKind::AND_EQUALS,
        TokenKind::PipeEquals => SyntaxKind::PIPE_EQUALS,
        TokenKind::CaretEquals => SyntaxKind::CARET_EQUALS,
        TokenKind::LessLessEquals => SyntaxKind::LESS_LESS_EQUALS,
        TokenKind::GreaterGreaterEquals => SyntaxKind::GREATER_GREATER_EQUALS,

        // Comparison operators
        TokenKind::EqualsEquals => SyntaxKind::EQUALS_EQUALS,
        TokenKind::NotEquals => SyntaxKind::NOT_EQUALS,
        TokenKind::Less => SyntaxKind::LESS,
        TokenKind::Greater => SyntaxKind::GREATER,
        TokenKind::LessEquals => SyntaxKind::LESS_EQUALS,
        TokenKind::GreaterEquals => SyntaxKind::GREATER_EQUALS,

        // Logical operators
        TokenKind::AndAnd => SyntaxKind::AND_AND,
        TokenKind::OrOr => SyntaxKind::OR_OR,
        TokenKind::Not => SyntaxKind::NOT,

        // Shift operators
        TokenKind::LessLess => SyntaxKind::LESS_LESS,
        TokenKind::GreaterGreater => SyntaxKind::GREATER_GREATER,

        // Bitwise operators
        TokenKind::And => SyntaxKind::AND,
        TokenKind::Caret => SyntaxKind::CARET,
        TokenKind::Tilde => SyntaxKind::TILDE,

        // Arithmetic operators
        TokenKind::Plus => SyntaxKind::PLUS,
        TokenKind::Minus => SyntaxKind::MINUS,
        TokenKind::Star => SyntaxKind::STAR,
        TokenKind::Slash => SyntaxKind::SLASH,
        TokenKind::Percent => SyntaxKind::PERCENT,
        TokenKind::PlusPlus => SyntaxKind::PLUS_PLUS,
        TokenKind::MinusMinus => SyntaxKind::MINUS_MINUS,

        // Whitespace and comments
        TokenKind::Whitespace => SyntaxKind::WHITESPACE,
        TokenKind::Newline => SyntaxKind::NEWLINE,
        TokenKind::LineComment => SyntaxKind::LINE_COMMENT,
        TokenKind::BlockComment => SyntaxKind::BLOCK_COMMENT,

        // Error
        TokenKind::Error => SyntaxKind::ERROR_TOKEN,
    }
}

/// Events for building the syntax tree.
#[derive(Debug, Clone)]
enum Event {
    StartNode { kind: SyntaxKind },
    FinishNode,
    Token { kind: SyntaxKind, text: String },
    Error { message: String },
}

/// Parser state for checkpoint/restore.
#[derive(Clone, Copy)]
struct ParserCheckpoint {
    current: usize,
    events_len: usize,
}

/// Recursive descent parser with error recovery.
pub(crate) struct Parser<'a> {
    tokens: &'a [Token],
    current: usize,
    events: Vec<Event>,
}

impl<'a> Parser<'a> {
    pub(crate) fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            current: 0,
            events: Vec::new(),
        }
    }

    // ============ Navigation ============

    /// Get current token (skipping trivia by default)
    fn current(&self) -> Option<&Token> {
        let mut i = self.current;
        while i < self.tokens.len() {
            let token = &self.tokens[i];
            if !self.is_trivia(token.kind) {
                return Some(token);
            }
            i += 1;
        }
        None
    }

    /// Peek ahead n tokens (skipping trivia)
    fn peek(&self, n: usize) -> Option<&Token> {
        let mut count = 0;
        let mut i = self.current;
        while i < self.tokens.len() {
            let token = &self.tokens[i];
            if !self.is_trivia(token.kind) {
                if count == n {
                    return Some(token);
                }
                count += 1;
            }
            i += 1;
        }
        None
    }

    /// Check if at end of input
    fn at_end(&self) -> bool {
        self.current().is_none()
    }

    /// Check if current token matches the given kind
    fn at(&self, kind: TokenKind) -> bool {
        self.current().map(|t| t.kind == kind).unwrap_or(false)
    }

    /// Check if current token is a keyword
    fn at_keyword(&self, keyword: &str) -> bool {
        self.current()
            .map(|t| t.kind == TokenKind::Word && t.text == keyword)
            .unwrap_or(false)
    }

    #[allow(clippy::unused_self)]
    fn is_trivia(&self, kind: TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Whitespace
                | TokenKind::Newline
                | TokenKind::LineComment
                | TokenKind::BlockComment
        )
    }

    // ============ Consumption ============

    /// Consume current token (including trivia before it)
    fn bump(&mut self) {
        // Emit any trivia before the token
        while self.current < self.tokens.len() {
            let token = &self.tokens[self.current];
            let kind = token_kind_to_syntax_kind(token.kind);

            self.events.push(Event::Token {
                kind,
                text: token.text.clone(),
            });

            self.current += 1;

            if !self.is_trivia(token.kind) {
                break;
            }
        }
    }

    /// Consume token if it matches expected kind
    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Consume keyword if text matches
    fn eat_keyword(&mut self, keyword: &str) -> bool {
        if self.at_keyword(keyword) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Expect a token, emit error if not found
    fn expect(&mut self, kind: TokenKind) -> bool {
        if self.eat(kind) {
            true
        } else {
            let found = self
                .current()
                .map(|t| format!("{:?}", t.kind))
                .unwrap_or_else(|| "EOF".to_string());
            self.error(format!("Expected {kind:?}, found {found}"));
            false
        }
    }

    /// Expect a keyword, emit error if not found
    fn expect_keyword(&mut self, keyword: &str) -> bool {
        if self.eat_keyword(keyword) {
            true
        } else {
            let found = self.current().map(|t| t.text.as_str()).unwrap_or("EOF");
            self.error(format!("Expected keyword '{keyword}', found '{found}'"));
            false
        }
    }

    // ============ Checkpoint Support for Speculative Parsing ============

    fn checkpoint(&self) -> ParserCheckpoint {
        ParserCheckpoint {
            current: self.current,
            events_len: self.events.len(),
        }
    }

    fn restore(&mut self, checkpoint: ParserCheckpoint) {
        self.current = checkpoint.current;
        self.events.truncate(checkpoint.events_len);
    }

    // ============ Tree Building ============

    fn start_node(&mut self, kind: SyntaxKind) {
        self.events.push(Event::StartNode { kind });
    }

    fn finish_node(&mut self) {
        self.events.push(Event::FinishNode);
    }

    fn error(&mut self, message: String) {
        self.events.push(Event::Error { message });
    }

    /// Parse with a node wrapper
    fn with_node<F>(&mut self, kind: SyntaxKind, f: F)
    where
        F: FnOnce(&mut Self),
    {
        self.start_node(kind);
        f(self);
        self.finish_node();
    }

    // ============ Building the Tree ============

    fn build_tree(self) -> (GreenNode, Vec<ParseError>) {
        let mut builder = GreenNodeBuilder::new();
        let mut errors = Vec::new();

        for event in self.events {
            match event {
                Event::StartNode { kind } => {
                    builder.start_node(kind.into());
                }
                Event::FinishNode => {
                    builder.finish_node();
                }
                Event::Token { kind, text } => {
                    builder.token(kind.into(), &text);
                }
                Event::Error { message } => {
                    // Store error for later reporting
                    // TODO: Need to track spans properly
                    errors.push(ParseError::UnexpectedToken {
                        expected: message.clone(),
                        found: message,
                        span: baml_base::Span::new(baml_base::FileId::new(0), TextRange::default()),
                    });
                }
            }
        }

        (builder.finish(), errors)
    }

    // ============ String Parsing ============

    /// Count consecutive Hash tokens starting at current position (raw skip trivia)
    fn count_consecutive_hashes(&self) -> usize {
        let mut count = 0;
        let mut i = self.current;

        while i < self.tokens.len() {
            let token = &self.tokens[i];
            if token.kind == TokenKind::Hash {
                count += 1;
                i += 1;
            } else if self.is_trivia(token.kind) {
                i += 1;
            } else {
                break;
            }
        }

        count
    }

    /// Find the token position after consuming N hashes (skipping trivia)
    fn find_token_after_hashes(&self, hash_count: usize) -> Option<usize> {
        let mut hashes_seen = 0;
        let mut i = self.current;

        while i < self.tokens.len() {
            let token = &self.tokens[i];
            if token.kind == TokenKind::Hash {
                hashes_seen += 1;
                i += 1;
                if hashes_seen == hash_count {
                    // Found all hashes, now skip trivia to find next token
                    while i < self.tokens.len() && self.is_trivia(self.tokens[i].kind) {
                        i += 1;
                    }
                    return Some(i);
                }
            } else if self.is_trivia(token.kind) {
                i += 1;
            } else {
                break;
            }
        }

        None
    }

    /// Count Hash tokens immediately after current Quote token (raw skip trivia)
    fn count_consecutive_hashes_after_quote(&self) -> usize {
        let mut count = 0;
        let mut i = self.current + 1;

        while i < self.tokens.len() {
            let token = &self.tokens[i];
            if token.kind == TokenKind::Hash {
                count += 1;
                i += 1;
            } else if self.is_trivia(token.kind) {
                i += 1;
            } else {
                break;
            }
        }

        count
    }

    /// Parse a string literal
    /// Lexer emits: Quote, (content tokens), Quote
    /// Parser assembles: `STRING_LITERAL` node
    pub(crate) fn parse_string(&mut self) -> bool {
        if !self.at(TokenKind::Quote) {
            return false;
        }

        self.with_node(SyntaxKind::STRING_LITERAL, |p| {
            p.bump(); // Opening quote

            // Collect all tokens until closing quote
            let mut depth = 1;
            while depth > 0 && !p.at_end() {
                if p.at(TokenKind::Quote) {
                    depth -= 1;
                }
                p.bump();
            }

            if depth != 0 {
                p.error("Unclosed string literal".to_string());
            }
        });

        true
    }

    /// Parse a raw string literal with hash delimiters
    /// Lexer emits: Hash+, Quote, (content tokens), Quote, Hash+
    /// Parser assembles and validates matching hash counts
    pub(crate) fn parse_raw_string(&mut self) -> bool {
        if !self.at(TokenKind::Hash) {
            return false;
        }

        // Count opening hashes
        let opening_hashes = self.count_consecutive_hashes();
        if opening_hashes == 0 {
            return false;
        }

        // Must be followed by opening quote - check after consuming hashes
        // We need to peek ahead past the hashes to see if there's a quote
        let quote_pos = self.find_token_after_hashes(opening_hashes);
        if quote_pos.is_none() || quote_pos.map(|i| self.tokens[i].kind) != Some(TokenKind::Quote) {
            // Just hashes, not a raw string
            return false;
        }

        self.with_node(SyntaxKind::RAW_STRING_LITERAL, |p| {
            // Consume opening hashes
            for _ in 0..opening_hashes {
                p.bump(); // #
            }
            p.bump(); // Opening "

            // Collect content until we find Quote followed by same number of hashes
            loop {
                if p.at_end() {
                    p.error(format!(
                        "Unclosed raw string (expected \"{}\")",
                        "#".repeat(opening_hashes)
                    ));
                    break;
                }

                if p.at(TokenKind::Quote) {
                    // Check if followed by correct number of hashes
                    let closing_hashes = p.count_consecutive_hashes_after_quote();
                    if closing_hashes == opening_hashes {
                        // Found matching closing delimiter
                        p.bump(); // Closing "
                        for _ in 0..closing_hashes {
                            p.bump(); // #
                        }
                        break;
                    }
                }

                // Not the closing delimiter, consume as content
                p.bump();
            }
        });

        true
    }

    /// Parse a string or raw string (dispatches to correct method)
    pub(crate) fn parse_any_string(&mut self) -> bool {
        if self.at(TokenKind::Hash) {
            self.parse_raw_string()
        } else if self.at(TokenKind::Quote) {
            self.parse_string()
        } else {
            false
        }
    }

    // ============ Attribute Parsing ============

    /// Parse a field attribute: @alias("name")
    pub(crate) fn parse_field_attribute(&mut self) {
        self.with_node(SyntaxKind::ATTRIBUTE, |p| {
            p.expect(TokenKind::At);

            // Attribute name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error("Expected attribute name".to_string());
                return;
            }

            // Optional arguments in parentheses
            if p.at(TokenKind::LParen) {
                p.parse_attribute_args();
            }
        });
    }

    /// Parse a block attribute: @@dynamic
    pub(crate) fn parse_block_attribute(&mut self) {
        self.with_node(SyntaxKind::BLOCK_ATTRIBUTE, |p| {
            p.expect(TokenKind::AtAt);

            // Attribute name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error("Expected attribute name".to_string());
                return;
            }

            // Optional arguments in parentheses
            if p.at(TokenKind::LParen) {
                p.parse_attribute_args();
            }
        });
    }

    fn parse_attribute_args(&mut self) {
        self.with_node(SyntaxKind::ATTRIBUTE_ARGS, |p| {
            p.expect(TokenKind::LParen);

            // Parse first argument
            if !p.at(TokenKind::RParen) {
                p.parse_attribute_arg();

                // Parse remaining arguments
                while p.eat(TokenKind::Comma) {
                    if p.at(TokenKind::RParen) {
                        break; // Trailing comma
                    }
                    p.parse_attribute_arg();
                }
            }

            p.expect(TokenKind::RParen);
        });
    }

    fn parse_attribute_arg(&mut self) {
        // Attribute argument can be:
        // - String: @alias("user_name")
        // - Raw string: @description(#"Multi-line\ndescription"#)
        // - Expression: @assert({{ this > 0 }})
        // - Identifier: @alias(field_name)

        if self.parse_any_string() {
            // String argument parsed
        } else if self.at(TokenKind::LBrace)
            && self
                .peek(1)
                .map(|t| t.kind == TokenKind::LBrace)
                .unwrap_or(false)
        {
            // Expression block: {{ }}
            self.parse_expression_block();
        } else if self.at(TokenKind::Word) {
            // Identifier or keyword
            self.bump();
        } else {
            self.error("Expected attribute argument".to_string());
        }
    }

    /// Placeholder for expression block parsing (Phase 4)
    fn parse_expression_block(&mut self) {
        // For now, just consume the {{ }} tokens
        self.with_node(SyntaxKind::EXPR, |p| {
            p.bump(); // {
            p.bump(); // {

            // Consume until }}
            while !p.at_end() {
                if p.at(TokenKind::RBrace)
                    && p.peek(1)
                        .map(|t| t.kind == TokenKind::RBrace)
                        .unwrap_or(false)
                {
                    p.bump(); // }
                    p.bump(); // }
                    break;
                }
                p.bump();
            }
        });
    }

    // ============ Type Parsing ============

    /// Parse a type expression
    /// Examples: string, int, User, string[], map<string, int>, string | int
    /// Can also use string literals: "user" | "assistant"
    pub(crate) fn parse_type(&mut self) {
        self.with_node(SyntaxKind::TYPE_EXPR, |p| {
            p.parse_type_primary();

            // Type modifiers
            loop {
                if p.at(TokenKind::LBracket) {
                    // Array type: string[]
                    p.bump(); // [
                    p.expect(TokenKind::RBracket); // ]
                } else if p.at(TokenKind::Question) {
                    // Optional type: string?
                    p.bump();
                } else if p.at(TokenKind::Pipe) {
                    // Union type: string | int | "user" | "assistant"
                    p.bump();
                    p.parse_type_primary();
                } else {
                    break;
                }
            }
        });
    }

    fn parse_type_primary(&mut self) {
        // Check for string literal types
        if self.parse_any_string() {
            // String literal type: "user" | "assistant"
            return;
        }

        if self.at(TokenKind::Word) {
            // Base type name or generic type
            self.bump();

            // Check for generic arguments: map<K, V>
            if self.at(TokenKind::Less) {
                self.with_node(SyntaxKind::TYPE_ARGS, |p| {
                    p.bump(); // <

                    p.parse_type();

                    while p.eat(TokenKind::Comma) {
                        p.parse_type();
                    }

                    p.expect(TokenKind::Greater);
                });
            }
        } else if self.at(TokenKind::LParen) {
            // Tuple type or parenthesized type
            self.bump(); // (
            self.parse_type();
            while self.eat(TokenKind::Comma) {
                self.parse_type();
            }
            self.expect(TokenKind::RParen);
        } else {
            self.error("Expected type".to_string());
        }
    }

    // ============ Enum Parsing ============

    /// Parse an enum declaration
    pub(crate) fn parse_enum(&mut self) {
        self.with_node(SyntaxKind::ENUM_DEF, |p| {
            // 'enum' keyword
            p.expect_keyword("enum");

            // Enum name
            if p.at(TokenKind::Word) {
                p.bump(); // name
            } else {
                p.error("Expected enum name".to_string());
            }

            // Opening brace
            if !p.expect(TokenKind::LBrace) {
                return; // Error recovery: stop here
            }

            // Parse enum variants and attributes
            while !p.at(TokenKind::RBrace) && !p.at_end() {
                if p.at(TokenKind::AtAt) {
                    // Block attribute: @@dynamic
                    p.parse_block_attribute();
                } else if p.at(TokenKind::Word) {
                    // Enum variant
                    p.parse_enum_variant();
                } else {
                    // Skip unexpected token
                    p.error("Unexpected token in enum body".to_string());
                    p.bump();
                }
            }

            // Closing brace
            p.expect(TokenKind::RBrace);
        });
    }

    fn parse_enum_variant(&mut self) {
        self.with_node(SyntaxKind::ENUM_VARIANT, |p| {
            // Variant name
            p.bump();

            // Optional field attributes (@alias, etc.)
            while p.at(TokenKind::At) && !p.at(TokenKind::AtAt) {
                p.parse_field_attribute();
            }
        });
    }

    // ============ Class Parsing ============

    /// Parse a class declaration
    pub(crate) fn parse_class(&mut self) {
        self.with_node(SyntaxKind::CLASS_DEF, |p| {
            // 'class' keyword
            p.expect_keyword("class");

            // Class name
            if p.at(TokenKind::Word) {
                p.bump(); // name
            } else {
                p.error("Expected class name".to_string());
            }

            // Opening brace
            if !p.expect(TokenKind::LBrace) {
                return;
            }

            // Parse fields and attributes
            while !p.at(TokenKind::RBrace) && !p.at_end() {
                if p.at(TokenKind::AtAt) {
                    // Block attribute: @@dynamic
                    p.parse_block_attribute();
                } else if p.at(TokenKind::Word) {
                    // Field declaration
                    p.parse_field();
                } else {
                    // Skip unexpected token
                    p.error("Unexpected token in class body".to_string());
                    p.bump();
                }
            }

            // Closing brace
            p.expect(TokenKind::RBrace);
        });
    }

    fn parse_field(&mut self) {
        self.with_node(SyntaxKind::FIELD, |p| {
            // Field name
            p.bump();

            // Field type
            p.parse_type();

            // Optional field attributes (@alias, @description, @assert, etc.)
            while p.at(TokenKind::At) && !p.at(TokenKind::AtAt) {
                p.parse_field_attribute();
            }
        });
    }

    // ============ Function Parsing ============

    /// Parse a function declaration with speculative parsing for body type
    pub(crate) fn parse_function(&mut self) {
        self.with_node(SyntaxKind::FUNCTION_DEF, |p| {
            // 'function' keyword
            p.expect_keyword("function");

            // Function name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error("Expected function name".to_string());
            }

            // Parameters
            p.parse_parameter_list();

            // Return type
            if p.eat(TokenKind::Arrow) {
                p.parse_type();
            } else {
                p.error("Expected return type (->)".to_string());
            }

            // Body
            if p.at(TokenKind::LBrace) {
                p.parse_function_body();
            } else {
                p.error("Expected function body".to_string());
            }
        });
    }

    fn parse_parameter_list(&mut self) {
        self.with_node(SyntaxKind::PARAMETER_LIST, |p| {
            p.expect(TokenKind::LParen);

            if !p.at(TokenKind::RParen) {
                p.parse_parameter();

                while p.eat(TokenKind::Comma) {
                    if p.at(TokenKind::RParen) {
                        break; // Trailing comma
                    }
                    p.parse_parameter();
                }
            }

            p.expect(TokenKind::RParen);
        });
    }

    fn parse_parameter(&mut self) {
        self.with_node(SyntaxKind::PARAMETER, |p| {
            // Parameter name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error("Expected parameter name".to_string());
            }

            // Type annotation
            if p.eat(TokenKind::Colon) {
                p.parse_type();
            } else {
                p.error("Expected type annotation (:)".to_string());
            }
        });
    }

    fn parse_function_body(&mut self) {
        // Use speculative parsing to determine function type
        let checkpoint = self.checkpoint();

        // Try parsing as LLM function
        let llm_errors = self.try_parse_as_llm_function();

        // Restore to checkpoint
        self.restore(checkpoint);

        // Try parsing as expression function
        let expr_errors = self.try_parse_as_expr_function();

        // Choose the interpretation with fewer errors
        let use_llm = self.should_use_llm_interpretation(&llm_errors, &expr_errors, &checkpoint);

        if use_llm {
            // Use LLM interpretation - restore and parse again as LLM
            self.restore(checkpoint);
            self.try_parse_as_llm_function();
        }
        // Otherwise, expression interpretation is already current
    }

    fn try_parse_as_llm_function(&mut self) -> Vec<String> {
        let mut errors = Vec::new();

        self.with_node(SyntaxKind::LLM_FUNCTION_BODY, |p| {
            p.expect(TokenKind::LBrace);

            let mut has_client = false;
            let mut has_prompt = false;

            while !p.at(TokenKind::RBrace) && !p.at_end() {
                if p.at_keyword("client") {
                    if has_client {
                        errors.push("Duplicate 'client' field".to_string());
                    }
                    has_client = true;
                    p.parse_client_field();
                } else if p.at_keyword("prompt") {
                    if has_prompt {
                        errors.push("Duplicate 'prompt' field".to_string());
                    }
                    has_prompt = true;
                    p.parse_prompt_field();
                } else {
                    // Unexpected token in LLM function
                    errors.push(format!(
                        "Only 'client' and 'prompt' allowed in LLM function, found '{}'",
                        p.current().map(|t| t.text.as_str()).unwrap_or("EOF")
                    ));
                    p.bump();
                }
            }

            if !has_client {
                errors.push("LLM function missing 'client' field".to_string());
            }
            if !has_prompt {
                errors.push("LLM function missing 'prompt' field".to_string());
            }

            p.expect(TokenKind::RBrace);
        });

        errors
    }

    fn try_parse_as_expr_function(&mut self) -> Vec<String> {
        let errors = Vec::new();

        self.with_node(SyntaxKind::EXPR_FUNCTION_BODY, |p| {
            p.parse_block_expr(); // Parse as a block expression with statements
        });

        errors
    }

    fn should_use_llm_interpretation(
        &self,
        llm_errors: &[String],
        expr_errors: &[String],
        checkpoint: &ParserCheckpoint,
    ) -> bool {
        // If error counts differ, choose the one with fewer errors
        if llm_errors.len() < expr_errors.len() {
            return true;
        }
        if expr_errors.len() < llm_errors.len() {
            return false;
        }

        // Tie-breaking heuristics when error counts are equal:
        // Look ahead from checkpoint to see if body contains LLM keywords
        let mut i = checkpoint.current;
        while i < self.tokens.len() {
            let token = &self.tokens[i];
            if token.kind == TokenKind::RBrace {
                break; // End of function body
            }
            if token.kind == TokenKind::Word {
                let text = &token.text;
                if text == "client" || text == "prompt" {
                    return true; // Has LLM keywords, prefer LLM interpretation
                }
                if text == "let"
                    || text == "return"
                    || text == "if"
                    || text == "while"
                    || text == "for"
                {
                    return false; // Has expression keywords, prefer expression interpretation
                }
            }
            i += 1;
        }

        // Default to expression function (more general)
        false
    }

    fn parse_client_field(&mut self) {
        self.with_node(SyntaxKind::CLIENT_FIELD, |p| {
            p.expect_keyword("client");

            // Client name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error("Expected client name".to_string());
            }
        });
    }

    fn parse_prompt_field(&mut self) {
        self.with_node(SyntaxKind::PROMPT_FIELD, |p| {
            p.expect_keyword("prompt");

            // Prompt value (usually a raw string)
            if !p.parse_any_string() {
                p.error("Expected prompt string".to_string());
            }
        });
    }

    /// Parse a block expression with statements
    fn parse_block_expr(&mut self) {
        self.with_node(SyntaxKind::BLOCK_EXPR, |p| {
            p.expect(TokenKind::LBrace);

            // Parse statements until closing brace
            while !p.at(TokenKind::RBrace) && !p.at_end() {
                p.parse_stmt();
            }

            p.expect(TokenKind::RBrace);
        });
    }

    // ============ Statement Parsing ============

    /// Parse a statement
    fn parse_stmt(&mut self) {
        if self.at_keyword("let") {
            self.parse_let_stmt();
        } else if self.at_keyword("return") {
            self.parse_return_stmt();
        } else if self.at_keyword("if") {
            self.parse_if_expr();
        } else if self.at_keyword("while") {
            self.parse_while_stmt();
        } else if self.at_keyword("for") {
            self.parse_for_expr();
        } else if self.at_keyword("break") {
            self.parse_break_stmt();
        } else if self.at_keyword("continue") {
            self.parse_continue_stmt();
        } else {
            // Expression statement
            self.parse_expr_stmt();
        }
    }

    fn parse_let_stmt(&mut self) {
        self.with_node(SyntaxKind::LET_STMT, |p| {
            p.expect_keyword("let");

            // Variable name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error("Expected variable name".to_string());
            }

            // Optional type annotation
            if p.eat(TokenKind::Colon) {
                p.parse_type();
            }

            // Initializer
            if p.eat(TokenKind::Equals) {
                // Parse expression but exclude assignment operators (parse_expr_bp with min_bp=3)
                // This prevents `let a = b = c` from being parsed as nested assignment
                p.parse_expr_bp(3);
            } else {
                p.error("Expected initializer (=)".to_string());
            }
        });
    }

    fn parse_return_stmt(&mut self) {
        self.with_node(SyntaxKind::RETURN_STMT, |p| {
            p.expect_keyword("return");

            // Optional return value
            if !p.at(TokenKind::RBrace) && !p.at_end() {
                p.parse_expr();
            }
        });
    }

    fn parse_if_expr(&mut self) {
        self.with_node(SyntaxKind::IF_EXPR, |p| {
            p.expect_keyword("if");

            // Condition
            p.parse_expr();

            // Then block
            if p.at(TokenKind::LBrace) {
                p.parse_block_expr();
            } else {
                p.error("Expected block after if condition".to_string());
            }

            // Optional else
            if p.at_keyword("else") {
                p.bump(); // else

                if p.at_keyword("if") {
                    // else if
                    p.parse_if_expr();
                } else if p.at(TokenKind::LBrace) {
                    // else block
                    p.parse_block_expr();
                } else {
                    p.error("Expected 'if' or block after 'else'".to_string());
                }
            }
        });
    }

    fn parse_while_stmt(&mut self) {
        self.with_node(SyntaxKind::WHILE_STMT, |p| {
            p.expect_keyword("while");

            // Condition
            p.parse_expr();

            // Body
            if p.at(TokenKind::LBrace) {
                p.parse_block_expr();
            } else {
                p.error("Expected block after while condition".to_string());
            }
        });
    }

    fn parse_for_expr(&mut self) {
        self.with_node(SyntaxKind::FOR_EXPR, |p| {
            p.expect_keyword("for");

            // Loop variable
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error("Expected loop variable".to_string());
            }

            // 'in' keyword
            p.expect_keyword("in");

            // Iterator expression
            p.parse_expr();

            // Body
            if p.at(TokenKind::LBrace) {
                p.parse_block_expr();
            } else {
                p.error("Expected block after for expression".to_string());
            }
        });
    }

    fn parse_break_stmt(&mut self) {
        self.with_node(SyntaxKind::BREAK_STMT, |p| {
            p.expect_keyword("break");
        });
    }

    fn parse_continue_stmt(&mut self) {
        self.with_node(SyntaxKind::CONTINUE_STMT, |p| {
            p.expect_keyword("continue");
        });
    }

    fn parse_expr_stmt(&mut self) {
        // Just an expression followed by optional semicolon
        self.parse_expr();
        self.eat(TokenKind::Semicolon); // Optional semicolon
    }

    // ============ Expression Parsing (Pratt Parser) ============

    /// Parse an expression with operator precedence
    fn parse_expr(&mut self) {
        self.parse_expr_bp(0);
    }

    /// Parse expression with binding power (Pratt parsing)
    fn parse_expr_bp(&mut self, min_bp: u8) {
        // Mark the start of this expression to prevent wrapping earlier tokens
        let expr_start = self.events.len();

        // Parse prefix (primary expression or unary operator)
        self.parse_prefix();

        // Parse infix operators
        while let Some(token) = self.current() {
            let op = token.kind;

            // Check if this is an infix operator
            if let Some((left_bp, right_bp)) = Self::infix_binding_power(op) {
                if left_bp < min_bp {
                    break;
                }

                // Mark where to start wrapping (before the LHS we just parsed)
                // but not before the expr_start marker
                let lhs_start = self.find_previous_expr_start_after(expr_start);

                // Consume the operator
                self.bump();

                // Parse right-hand side
                self.parse_expr_bp(right_bp);

                // Wrap everything from lhs_start in a BINARY_EXPR
                self.wrap_events_in_node(lhs_start, SyntaxKind::BINARY_EXPR);
                self.finish_node();
            } else if op == TokenKind::LParen {
                // Function call
                let lhs_start = self.find_previous_expr_start_after(expr_start);
                self.wrap_events_in_node(lhs_start, SyntaxKind::CALL_EXPR);
                self.parse_call_args();
                self.finish_node();
            } else if op == TokenKind::LBracket {
                // Index expression
                let lhs_start = self.find_previous_expr_start_after(expr_start);
                self.wrap_events_in_node(lhs_start, SyntaxKind::INDEX_EXPR);
                self.bump(); // [
                self.parse_expr();
                self.expect(TokenKind::RBracket);
                self.finish_node();
            } else if op == TokenKind::Dot {
                // Field access
                let lhs_start = self.find_previous_expr_start_after(expr_start);
                self.wrap_events_in_node(lhs_start, SyntaxKind::FIELD_ACCESS_EXPR);
                self.bump(); // .
                if self.at(TokenKind::Word) {
                    self.bump();
                } else {
                    self.error("Expected field name after '.'".to_string());
                }
                self.finish_node();
            } else if op == TokenKind::LBrace {
                // Object literal/constructor
                // Check if we have a preceding expression (constructor name/expression)
                // by checking if we've emitted any events since expr_start
                if self.events.len() > expr_start {
                    // We have a preceding expression, treat as object literal/constructor
                    let lhs_start = self.find_previous_expr_start_after(expr_start);
                    self.wrap_events_in_node(lhs_start, SyntaxKind::OBJECT_LITERAL);
                    self.parse_object_literal_body();
                    self.finish_node();
                } else {
                    // No preceding expression, this is a block expression
                    // Break and let parse_primary_expr handle it
                    break;
                }
            } else {
                break;
            }
        }
    }

    /// Find the start of the most recent complete expression, but not before `min_index`
    /// This walks backward through events to find where the last expression began
    fn find_previous_expr_start_after(&self, min_index: usize) -> usize {
        let mut depth = 0;
        let mut i = self.events.len();

        while i > min_index {
            i -= 1;
            match &self.events[i] {
                Event::FinishNode => depth += 1,
                Event::StartNode { .. } => {
                    if depth == 0 {
                        return i;
                    }
                    depth -= 1;
                }
                Event::Token { .. } => {
                    if depth == 0 {
                        return i;
                    }
                }
                Event::Error { .. } => {}
            }
        }

        min_index
    }

    /// Wrap events from `start_index` onwards in a new node
    /// This allows us to retroactively wrap parsed expressions.
    ///
    /// For example, in an expression like `a + b`, the parser will
    /// parse `a` before seeing the binary operator that triggers
    /// binary expression parsing, so we need this function to
    /// reassociate the event from that previous expression into
    /// the binary expression node.
    fn wrap_events_in_node(&mut self, start_index: usize, kind: SyntaxKind) {
        // Insert StartNode at the beginning
        self.events.insert(start_index, Event::StartNode { kind });
    }

    /// Parse prefix expression (primary or unary operator)
    fn parse_prefix(&mut self) {
        // Check for unary operators
        if self.at(TokenKind::Minus)
            || self.at(TokenKind::Not)
            || self.at(TokenKind::Tilde)
            || self.at(TokenKind::PlusPlus)
            || self.at(TokenKind::MinusMinus)
        {
            self.with_node(SyntaxKind::UNARY_EXPR, |p| {
                p.bump(); // operator
                p.parse_prefix(); // operand
            });
        } else {
            self.parse_primary_expr();
        }
    }

    /// Parse primary expression (literals, identifiers, parentheses)
    fn parse_primary_expr(&mut self) {
        if self.at(TokenKind::Integer) || self.at(TokenKind::Float) {
            // Numeric literal
            self.bump();
        } else if self.parse_any_string() {
            // String literal
        } else if self.at_keyword("true") || self.at_keyword("false") {
            // Boolean literal
            self.bump();
        } else if self.at_keyword("null") {
            // Null literal
            self.bump();
        } else if self.at(TokenKind::Word) {
            // Identifier or path
            self.bump();
        } else if self.at(TokenKind::LParen) {
            // Parenthesized expression
            self.with_node(SyntaxKind::PAREN_EXPR, |p| {
                p.bump(); // (
                p.parse_expr();
                p.expect(TokenKind::RParen);
            });
        } else if self.at(TokenKind::LBracket) {
            // Array literal
            self.parse_array_literal();
        } else if self.at(TokenKind::LBrace) {
            // Block expression (not object literal - that's handled in parse_expr_bp)
            // This only triggers for blocks at expression start position
            self.parse_block_expr();
        } else {
            self.error("Expected expression".to_string());
            self.bump(); // Consume unexpected token.
        }
    }

    fn parse_call_args(&mut self) {
        self.with_node(SyntaxKind::CALL_ARGS, |p| {
            p.expect(TokenKind::LParen);

            if !p.at(TokenKind::RParen) {
                p.parse_expr();

                while p.eat(TokenKind::Comma) {
                    if p.at(TokenKind::RParen) {
                        break; // Trailing comma
                    }
                    p.parse_expr();
                }
            }

            p.expect(TokenKind::RParen);
        });
    }

    fn parse_array_literal(&mut self) {
        self.with_node(SyntaxKind::ARRAY_LITERAL, |p| {
            p.expect(TokenKind::LBracket);

            if !p.at(TokenKind::RBracket) {
                p.parse_expr();

                while p.eat(TokenKind::Comma) {
                    if p.at(TokenKind::RBracket) {
                        break; // Trailing comma
                    }
                    p.parse_expr();
                }
            }

            p.expect(TokenKind::RBracket);
        });
    }

    /// Parse the body of an object literal/constructor: { field: value, ... }
    fn parse_object_literal_body(&mut self) {
        self.expect(TokenKind::LBrace);

        // Parse fields until we hit the closing brace
        while !self.at(TokenKind::RBrace) && !self.at_end() {
            // Check for valid field start
            if self.at(TokenKind::Word) || self.at(TokenKind::Quote) || self.at(TokenKind::Hash) {
                self.parse_object_field();

                // Handle comma between fields
                if !self.at(TokenKind::RBrace) {
                    if !self.eat(TokenKind::Comma) {
                        // Missing comma - error but try to continue
                        self.error("Expected ',' or '}' after object field".to_string());
                        // Try to recover by looking for next field or closing brace
                        if !self.at(TokenKind::Word)
                            && !self.at(TokenKind::Quote)
                            && !self.at(TokenKind::Hash)
                            && !self.at(TokenKind::RBrace)
                        {
                            // Skip unexpected token
                            self.bump();
                        }
                    }
                }
            } else if self.eat(TokenKind::Comma) {
                // Trailing comma or double comma - just continue
                continue;
            } else {
                // Unexpected token in object literal
                self.error("Expected field name or '}'".to_string());
                // Skip the unexpected token to avoid getting stuck
                self.bump();
            }
        }

        self.expect(TokenKind::RBrace);
    }

    /// Parse a single object field: name: value
    fn parse_object_field(&mut self) {
        self.with_node(SyntaxKind::OBJECT_FIELD, |p| {
            // Field name - can be identifier or string literal
            if p.at(TokenKind::Word) {
                p.bump(); // identifier field name
            } else if !p.parse_any_string() {
                p.error("Expected field name".to_string());
                return;
            }

            // Colon
            if !p.expect(TokenKind::Colon) {
                return; // Error already emitted by expect
            }

            // Field value - any expression (including nested constructors)
            p.parse_expr();
        });
    }

    /// Get infix operator binding power (precedence)
    /// Returns (`left_bp`, `right_bp`) for left and right associativity
    fn infix_binding_power(op: TokenKind) -> Option<(u8, u8)> {
        use TokenKind::{
            And, AndAnd, AndEquals, Caret, CaretEquals, Equals, EqualsEquals, Greater,
            GreaterEquals, GreaterGreater, GreaterGreaterEquals, Less, LessEquals, LessLess,
            LessLessEquals, Minus, MinusEquals, NotEquals, OrOr, Percent, PercentEquals, Pipe,
            PipeEquals, Plus, PlusEquals, Slash, SlashEquals, Star, StarEquals,
        };

        Some(match op {
            // Assignment operators (right associative)
            Equals | PlusEquals | MinusEquals | StarEquals | SlashEquals | PercentEquals
            | AndEquals | PipeEquals | CaretEquals | LessLessEquals | GreaterGreaterEquals => {
                (2, 1)
            }

            // Logical OR (left associative)
            OrOr => (3, 4),

            // Logical AND (left associative)
            AndAnd => (5, 6),

            // Bitwise OR (left associative)
            Pipe => (7, 8),

            // Bitwise XOR (left associative)
            Caret => (9, 10),

            // Bitwise AND (left associative)
            And => (11, 12),

            // Equality (left associative)
            EqualsEquals | NotEquals => (13, 14),

            // Comparison (left associative)
            Less | Greater | LessEquals | GreaterEquals => (15, 16),

            // Bitwise shift (left associative)
            LessLess | GreaterGreater => (17, 18),

            // Addition/Subtraction (left associative)
            Plus | Minus => (19, 20),

            // Multiplication/Division/Modulo (left associative)
            Star | Slash | Percent => (21, 22),

            _ => return None,
        })
    }

    // ============ Client Parsing ============

    /// Parse a client declaration
    pub(crate) fn parse_client(&mut self) {
        self.with_node(SyntaxKind::CLIENT_DEF, |p| {
            // 'client' keyword
            p.expect_keyword("client");

            // Optional client type: <llm>
            if p.at(TokenKind::Less) {
                p.with_node(SyntaxKind::CLIENT_TYPE, |p| {
                    p.bump(); // <
                    if p.at(TokenKind::Word) {
                        p.bump(); // type name
                    }
                    p.expect(TokenKind::Greater); // >
                });
            }

            // Client name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error("Expected client name".to_string());
            }

            // Config block
            if p.at(TokenKind::LBrace) {
                p.parse_config_block();
            } else {
                p.error("Expected config block".to_string());
            }
        });
    }

    fn parse_config_block(&mut self) {
        self.with_node(SyntaxKind::CONFIG_BLOCK, |p| {
            p.expect(TokenKind::LBrace);

            while !p.at(TokenKind::RBrace) && !p.at_end() {
                p.parse_config_item();
            }

            p.expect(TokenKind::RBrace);
        });
    }

    fn parse_config_item(&mut self) {
        self.with_node(SyntaxKind::CONFIG_ITEM, |p| {
            // Config key (identifier)
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error("Expected config key".to_string());
                return;
            }

            // Config value - can be nested block or simple value
            if p.at(TokenKind::LBrace) {
                // Nested config block
                p.parse_config_block();
            } else {
                // Simple value - unquoted string or other expression
                p.parse_config_value();
            }
        });
    }

    fn parse_config_value(&mut self) {
        self.with_node(SyntaxKind::CONFIG_VALUE, |p| {
            // Config values can be:
            // - Strings: "value"
            // - Raw strings: #"value"#
            // - Unquoted strings: gpt-4o, env.OPENAI_API_KEY
            // - Numbers: 123, 3.14

            if p.parse_any_string() {
                // String value
                return;
            }

            // Parse unquoted string - consume tokens until newline, comma, or brace
            while !p.at_end() {
                if p.at(TokenKind::Newline)
                    || p.at(TokenKind::Comma)
                    || p.at(TokenKind::RBrace)
                    || p.at(TokenKind::LBrace)
                {
                    break;
                }
                p.bump();
            }
        });
    }

    // ============ Test Parsing ============

    /// Parse a test declaration
    pub(crate) fn parse_test(&mut self) {
        self.with_node(SyntaxKind::TEST_DEF, |p| {
            // 'test' keyword
            p.expect_keyword("test");

            // Test name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error("Expected test name".to_string());
            }

            // Config block
            if p.at(TokenKind::LBrace) {
                p.parse_config_block();
            } else {
                p.error("Expected test body".to_string());
            }
        });
    }

    // ============ Retry Policy Parsing ============

    /// Parse a retry policy declaration
    pub(crate) fn parse_retry_policy(&mut self) {
        self.with_node(SyntaxKind::RETRY_POLICY_DEF, |p| {
            // 'retry_policy' keyword
            p.expect_keyword("retry_policy");

            // Policy name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error("Expected retry policy name".to_string());
            }

            // Config block
            if p.at(TokenKind::LBrace) {
                p.parse_config_block();
            } else {
                p.error("Expected retry policy body".to_string());
            }
        });
    }

    // ============ Template String Parsing ============

    /// Parse a template string declaration
    pub(crate) fn parse_template_string(&mut self) {
        self.with_node(SyntaxKind::TEMPLATE_STRING_DEF, |p| {
            // 'template_string' keyword
            p.expect_keyword("template_string");

            // Template name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error("Expected template string name".to_string());
            }

            // Parameters
            p.parse_parameter_list();

            // Template body (raw string)
            if !p.parse_any_string() {
                p.error("Expected template string body".to_string());
            }
        });
    }

    // ============ Type Alias Parsing ============

    /// Parse a type alias declaration
    pub(crate) fn parse_type_alias(&mut self) {
        self.with_node(SyntaxKind::TYPE_ALIAS_DEF, |p| {
            // 'type' keyword
            p.expect_keyword("type");

            // Type alias name
            if p.at(TokenKind::Word) {
                p.bump();
            } else {
                p.error("Expected type alias name".to_string());
            }

            // Equals
            p.expect(TokenKind::Equals);

            // Type definition
            p.parse_type();

            // Optional attributes
            while p.at(TokenKind::At) && !p.at(TokenKind::AtAt) {
                p.parse_field_attribute();
            }
        });
    }
}

/// Parse tokens into a green tree.
///
/// Returns the green tree and any parse errors encountered.
pub fn parse_file(tokens: &[Token]) -> (GreenNode, Vec<ParseError>) {
    let mut parser = Parser::new(tokens);

    parser.start_node(SyntaxKind::SOURCE_FILE);

    // Parse top-level declarations
    while !parser.at_end() {
        if parser.at_keyword("enum") {
            parser.parse_enum();
        } else if parser.at_keyword("class") {
            parser.parse_class();
        } else if parser.at_keyword("function") {
            parser.parse_function();
        } else if parser.at_keyword("client") {
            parser.parse_client();
        } else if parser.at_keyword("test") {
            parser.parse_test();
        } else if parser.at_keyword("retry_policy") {
            parser.parse_retry_policy();
        } else if parser.at_keyword("template_string") {
            parser.parse_template_string();
        } else if parser.at_keyword("type") {
            parser.parse_type_alias();
        } else {
            // Unknown top-level item - error recovery
            parser.error("Expected top-level declaration".to_string());
            parser.bump(); // Skip unknown token
        }
    }

    // Consume any remaining trailing trivia (whitespace, comments, newlines)
    // at_end() skips trivia, so we need to explicitly consume it for lossless parsing
    while parser.current < parser.tokens.len() {
        let token = &parser.tokens[parser.current];
        let kind = token_kind_to_syntax_kind(token.kind);
        parser.events.push(Event::Token {
            kind,
            text: token.text.clone(),
        });
        parser.current += 1;
    }

    parser.finish_node();

    parser.build_tree()
}
