//! The built-in hot swap rules (BEP-087, stage 2), and the `/// baml:hotswap`
//! mark that names one.
//!
//! A rule sees a swappable function's source as built and its replacement,
//! and accepts by returning `Ok` or rejects with a message for the publisher.
//! Every rule runs on tokens, so formatting and plain comments never matter.
//! Rules are defense in depth: approval and the audit trail still apply.

use baml_syntax::token::TokenKind;

pub use baml_syntax::hotswap::{Mark, Rule, parse_mark};

/// The mark on a declaration's source text, from the comments that lead it.
pub fn mark_of(source: &str) -> Option<Result<Mark, String>> {
    tokens(source)
        .into_iter()
        .take_while(|token| is_comment(token.kind))
        .find_map(|token| parse_mark(token.text))
}

/// A significant token: trivia dropped, docs and directives kept.
#[derive(Debug, Clone, Copy)]
struct Token<'s> {
    kind: TokenKind,
    text: &'s str,
    start: usize,
}

impl PartialEq for Token<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.text == other.text
    }
}

fn tokens(source: &str) -> Vec<Token<'_>> {
    let lexed = baml_syntax::lexer::lex(source);
    lexed
        .iter()
        .filter(|idx| !lexed.kind(*idx).is_trivia())
        .map(|idx| Token {
            kind: lexed.kind(idx),
            // A comment's trailing spaces are formatting.
            text: match lexed.kind(idx) {
                TokenKind::DocComment | TokenKind::DirectiveComment => {
                    lexed.text(source, idx).trim_end()
                }
                _ => lexed.text(source, idx),
            },
            start: lexed.range(idx).start().into(),
        })
        .collect()
}

fn is_comment(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::DocComment | TokenKind::DirectiveComment)
}

fn is_trace_directive(token: &Token<'_>) -> bool {
    is_comment(token.kind)
        && token
            .text
            .trim_start_matches('/')
            .trim()
            .starts_with("baml:$trace")
}

/// What a rule checks: one swappable function's source as built and its
/// replacement's, each the whole declaration with its leading comments.
pub struct Change<'a> {
    pub original: &'a str,
    pub replacement: &'a str,
    /// Clients the swap changes that this function uses. A client's new
    /// configuration counts as a change to every function that uses it.
    pub changed_clients: &'a [String],
}

/// Run `rule` on `change`. `shipped` returns the source as built of a
/// function `select_only` names, by the name the mark gives it.
pub fn check(
    rule: &Rule,
    change: &Change<'_>,
    shipped: &dyn Fn(&str) -> Option<String>,
) -> Result<(), String> {
    if !matches!(rule, Rule::Any)
        && let Some(client) = change.changed_clients.first()
    {
        return Err(format!(
            "client `{client}` changed, and only hotswap.any accepts a client change"
        ));
    }
    match rule {
        Rule::Any => Ok(()),
        Rule::SelectOnly(names) => select_only(names, change, shipped),
        Rule::TelemetryOnly => telemetry_only(change),
        Rule::TextOnly => text_only(change),
        Rule::NoNewCalls => no_new_calls(change),
    }
}

/// The tokens from `function` on, without the leading comments and without
/// the function's own name, so two functions compare by code alone.
fn code_without_name<'s>(tokens: &[Token<'s>]) -> Vec<Token<'s>> {
    let mut code: Vec<Token<'s>> = tokens
        .iter()
        .copied()
        .skip_while(|token| is_comment(token.kind))
        .collect();
    if let Some(function) = code
        .iter()
        .position(|token| token.kind == TokenKind::Function)
        && code
            .get(function + 1)
            .is_some_and(|token| token.kind == TokenKind::Word)
    {
        code.remove(function + 1);
    }
    code
}

fn select_only(
    names: &[String],
    change: &Change<'_>,
    shipped: &dyn Fn(&str) -> Option<String>,
) -> Result<(), String> {
    let replacement = code_without_name(&tokens(change.replacement));
    for name in names {
        let Some(source) = shipped(name) else {
            continue;
        };
        if code_without_name(&tokens(&source)) == replacement {
            return Ok(());
        }
    }
    let listed: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    Err(format!(
        "the replacement's code matches none of {}",
        listed.join(", ")
    ))
}

/// Whether the token before `index` ends an expression or a statement, so a
/// call that starts at `index` is a statement of its own, not a value.
fn starts_statement(tokens: &[Token<'_>], index: usize) -> bool {
    let Some(previous) = index.checked_sub(1).map(|i| tokens[i]) else {
        return true;
    };
    matches!(
        previous.kind,
        TokenKind::LBrace
            | TokenKind::RBrace
            | TokenKind::Semicolon
            | TokenKind::RParen
            | TokenKind::RBracket
            | TokenKind::Word
            | TokenKind::Integer
            | TokenKind::Float
            | TokenKind::String
            | TokenKind::RawString
            | TokenKind::BacktickEnd
            | TokenKind::True
            | TokenKind::False
            | TokenKind::Null
    )
}

/// The index past the bracket that closes the one at `open`.
fn past_close(tokens: &[Token<'_>], open: usize) -> usize {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        match token.kind {
            TokenKind::LParen
            | TokenKind::LBracket
            | TokenKind::LBrace
            | TokenKind::DollarLBrace => depth += 1,
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
    }
    tokens.len()
}

/// `tokens` without what telemetry adds: `log.<level>(...)` statements,
/// `$trace = ...` arguments, and `$trace` hook directives.
fn without_telemetry<'s>(tokens: &[Token<'s>]) -> Vec<Token<'s>> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if is_trace_directive(&token) {
            index += 1;
            continue;
        }
        let is_log_call = token.kind == TokenKind::Word
            && token.text == "log"
            && tokens
                .get(index + 1)
                .is_some_and(|t| t.kind == TokenKind::Dot)
            && tokens
                .get(index + 2)
                .is_some_and(|t| t.kind == TokenKind::Word)
            && tokens
                .get(index + 3)
                .is_some_and(|t| t.kind == TokenKind::LParen)
            && starts_statement(tokens, index);
        if is_log_call {
            index = past_close(tokens, index + 3);
            if tokens
                .get(index)
                .is_some_and(|t| t.kind == TokenKind::Semicolon)
            {
                index += 1;
            }
            continue;
        }
        let is_trace_argument = token.kind == TokenKind::Word
            && token.text == "$trace"
            && tokens
                .get(index + 1)
                .is_some_and(|t| t.kind == TokenKind::Equals);
        if is_trace_argument {
            // The argument runs to the next `,` or `)` at its own depth.
            let mut end = index + 2;
            while end < tokens.len() {
                match tokens[end].kind {
                    TokenKind::Comma | TokenKind::RParen => break,
                    TokenKind::LParen
                    | TokenKind::LBracket
                    | TokenKind::LBrace
                    | TokenKind::DollarLBrace => end = past_close(tokens, end),
                    _ => end += 1,
                }
            }
            if out
                .last()
                .is_some_and(|t: &Token<'_>| t.kind == TokenKind::Comma)
            {
                out.pop();
            } else if tokens.get(end).is_some_and(|t| t.kind == TokenKind::Comma) {
                end += 1;
            }
            index = end;
            continue;
        }
        out.push(token);
        index += 1;
    }
    out
}

fn telemetry_only(change: &Change<'_>) -> Result<(), String> {
    if without_telemetry(&tokens(change.original)) == without_telemetry(&tokens(change.replacement))
    {
        Ok(())
    } else {
        Err("the change does more than add telemetry".to_string())
    }
}

/// A function as `text_only` sees it.
struct Text<'s> {
    /// Every token, with the static text of each string and template literal
    /// left out, and each template's `${...}` too.
    skeleton: Vec<Token<'s>>,
    /// Each `${...}`, as written.
    expressions: Vec<String>,
    /// The same, by tokens, to compare without formatting.
    expression_tokens: Vec<Vec<Token<'s>>>,
}

fn text_of<'s>(source: &'s str, tokens: &[Token<'s>]) -> Text<'s> {
    let mut text = Text {
        skeleton: Vec::with_capacity(tokens.len()),
        expressions: Vec::new(),
        expression_tokens: Vec::new(),
    };
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        match token.kind {
            // A literal's text is no code: it stands as its kind alone.
            TokenKind::String | TokenKind::RawString => {
                text.skeleton.push(Token { text: "", ..token });
                index += 1;
            }
            TokenKind::BacktickStart => {
                text.skeleton.push(Token { text: "`", ..token });
                index += 1;
                while index < tokens.len() && tokens[index].kind != TokenKind::BacktickEnd {
                    if tokens[index].kind == TokenKind::DollarLBrace {
                        let end = past_close(tokens, index);
                        let from = tokens[index].start;
                        let to = tokens[end - 1].start + tokens[end - 1].text.len();
                        text.expressions.push(source[from..to].to_string());
                        text.expression_tokens.push(tokens[index..end].to_vec());
                        index = end;
                    } else {
                        index += 1;
                    }
                }
                if let Some(end) = tokens.get(index) {
                    text.skeleton.push(Token { text: "`", ..*end });
                }
                index += 1;
            }
            _ => {
                text.skeleton.push(token);
                index += 1;
            }
        }
    }
    text
}

/// The client an LLM function names, as tokens.
fn client_of<'s>(tokens: &[Token<'s>]) -> Vec<Token<'s>> {
    tokens
        .windows(2)
        .position(|pair| pair[0].kind == TokenKind::Client && pair[1].kind == TokenKind::Colon)
        .map(|at| {
            tokens[at + 2..]
                .iter()
                .copied()
                .take_while(|token| {
                    !matches!(
                        token.kind,
                        TokenKind::Prompt | TokenKind::RBrace | TokenKind::Comma
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Only the static text of string and template literals changes, a prompt's
/// included, and no `${...}` is new, so the change runs no new code. Text can
/// still change behavior, such as a string the code compares or a URL.
fn text_only(change: &Change<'_>) -> Result<(), String> {
    let (original_tokens, replacement_tokens) =
        (tokens(change.original), tokens(change.replacement));
    if client_of(&original_tokens) != client_of(&replacement_tokens) {
        return Err("the change edits `client`".to_string());
    }
    let original = text_of(change.original, &original_tokens);
    let replacement = text_of(change.replacement, &replacement_tokens);
    if original.skeleton != replacement.skeleton {
        return Err("the change edits more than static text".to_string());
    }
    for (text, expression) in replacement
        .expressions
        .iter()
        .zip(&replacement.expression_tokens)
    {
        if !original.expression_tokens.contains(expression) {
            return Err(format!("the change adds an expression, `{text}`"));
        }
    }
    Ok(())
}

/// Everything `tokens` calls, by the path written before `(`, and the client
/// an LLM function names, since a new client is a new destination.
fn callees(tokens: &[Token<'_>]) -> std::collections::BTreeSet<String> {
    let mut found = std::collections::BTreeSet::new();
    for (index, token) in tokens.iter().enumerate() {
        if token.kind == TokenKind::Client
            && tokens
                .get(index + 1)
                .is_some_and(|t| t.kind == TokenKind::Colon)
            && let Some(client) = tokens.get(index + 2).filter(|t| t.kind == TokenKind::Word)
        {
            found.insert(client.text.to_string());
        }
        if token.kind != TokenKind::LParen || index == 0 {
            continue;
        }
        // Skip a call's type arguments, `f<T>(...)`.
        let mut end = index;
        if tokens[end - 1].kind == TokenKind::Greater {
            let mut depth = 0usize;
            while end > 0 {
                end -= 1;
                match tokens[end].kind {
                    TokenKind::Greater => depth += 1,
                    TokenKind::Less => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
        if end == 0 || tokens[end - 1].kind != TokenKind::Word {
            continue;
        }
        let mut begin = end - 1;
        while begin >= 2
            && matches!(
                tokens[begin - 1].kind,
                TokenKind::Dot | TokenKind::QuestionDot
            )
            && tokens[begin - 2].kind == TokenKind::Word
        {
            begin -= 2;
        }
        // The function's own name is a declaration, not a call.
        if begin > 0 && tokens[begin - 1].kind == TokenKind::Function {
            continue;
        }
        let path: String = tokens[begin..end]
            .iter()
            .map(|token| match token.kind {
                TokenKind::QuestionDot => ".",
                _ => token.text,
            })
            .collect();
        found.insert(path);
    }
    found
}

fn no_new_calls(change: &Change<'_>) -> Result<(), String> {
    let original = callees(&tokens(change.original));
    let new: Vec<String> = callees(&tokens(change.replacement))
        .into_iter()
        .filter(|callee| !original.contains(callee))
        .map(|callee| format!("`{callee}`"))
        .collect();
    if new.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the replacement calls {}, which the original does not",
            new.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(rule: &Rule, original: &str, replacement: &str) -> Result<(), String> {
        let shipped = |name: &str| {
            match name {
            "RouteTicketByIntent" => Some(
                "function RouteTicketByIntent(text: string) -> string { Classify(text) }"
                    .to_string(),
            ),
            "RouteTicketByKeywords" => Some(
                "function RouteTicketByKeywords(text: string) -> string { if (text.contains(\"refund\")) { \"billing\" } else { \"general\" } }".to_string(),
            ),
            _ => None,
        }
        };
        check(
            rule,
            &Change {
                original,
                replacement,
                changed_clients: &[],
            },
            &shipped,
        )
    }

    #[test]
    fn marks_name_a_rule_or_none() {
        assert_eq!(
            parse_mark("/// baml:hotswap"),
            Some(Ok(Mark { rule: None }))
        );
        assert_eq!(
            parse_mark("//baml:hotswap=hotswap.text_only"),
            Some(Ok(Mark {
                rule: Some(Rule::TextOnly)
            }))
        );
        assert_eq!(
            parse_mark("/// baml:hotswap=hotswap.select_only([A, ns.B])"),
            Some(Ok(Mark {
                rule: Some(Rule::SelectOnly(vec!["A".into(), "ns.B".into()]))
            }))
        );
        assert_eq!(parse_mark("/// baml:$trace=hook"), None);
        assert_eq!(parse_mark("/// baml:hotswapping"), None);
        assert!(parse_mark("/// baml:hotswap=my_rule").unwrap().is_err());
        assert!(
            parse_mark("/// baml:hotswap=hotswap.select_only")
                .unwrap()
                .is_err()
        );
        assert_eq!(
            mark_of("/// Docs.\n/// baml:hotswap=hotswap.any\nfunction f() -> int { 1 }"),
            Some(Ok(Mark {
                rule: Some(Rule::Any)
            }))
        );
        assert_eq!(mark_of("function f() -> int { 1 }"), None);
    }

    const ROUTE: &str = "/// baml:hotswap=hotswap.select_only([RouteTicketByIntent, RouteTicketByKeywords])\nfunction RouteTicket(text: string) -> string { \"general\" }";

    #[test]
    fn select_only_accepts_a_shipped_function_by_code() {
        let rule = Rule::parse("hotswap.select_only([RouteTicketByIntent, RouteTicketByKeywords])")
            .unwrap();
        let by_intent = "/// baml:hotswap=hotswap.select_only([RouteTicketByIntent, RouteTicketByKeywords])\nfunction RouteTicket(text: string) -> string {\n  Classify(text)\n}";
        assert_eq!(run(&rule, ROUTE, by_intent), Ok(()));
        // An edited copy is rejected, though it names the same callee.
        let edited = by_intent.replace("Classify(text)", "Classify(text.lower())");
        assert_eq!(
            run(&rule, ROUTE, &edited),
            Err("the replacement's code matches none of `RouteTicketByIntent`, `RouteTicketByKeywords`".to_string())
        );
        // Calling the shipped function is not its code either.
        let calls = ROUTE.replace("\"general\"", "RouteTicketByIntent(text)");
        assert!(run(&rule, ROUTE, &calls).is_err());
    }

    const TRIAGE: &str = "/// baml:hotswap=hotswap.telemetry_only\nfunction Triage(ticket: Ticket) -> Queue {\n  let intent = Classify(ticket.text)\n  if (intent == \"refund\") { Queue.Billing } else { Queue.General }\n}";

    #[test]
    fn telemetry_only_accepts_log_lines_and_trace_options() {
        let rule = Rule::TelemetryOnly;
        let added = TRIAGE
            .replace(
                "  let intent = Classify(ticket.text)",
                "  log.info(`triaging ${ticket.id}`)\n  let intent = Classify(ticket.text, $trace = trace.span(output = true))",
            )
            .replace(
                "/// baml:hotswap=hotswap.telemetry_only",
                "/// baml:hotswap=hotswap.telemetry_only\n/// baml:$trace=decide",
            );
        assert_eq!(run(&rule, TRIAGE, &added), Ok(()));
        let behavior = TRIAGE.replace(
            "intent == \"refund\"",
            "intent == \"refund\" || intent == \"chargeback\"",
        );
        assert_eq!(
            run(&rule, TRIAGE, &behavior),
            Err("the change does more than add telemetry".to_string())
        );
        // A log call used as a value is not a log line.
        let as_value = TRIAGE.replace(
            "let intent = Classify(ticket.text)",
            "let intent = log.info(ticket.text)",
        );
        assert!(run(&rule, TRIAGE, &as_value).is_err());
    }

    const SUMMARIZE: &str = "/// baml:hotswap=hotswap.text_only\nfunction Summarize(doc: string) -> Summary {\n  client: Writer\n  prompt: `Summarize this document: ${doc}`\n}";

    #[test]
    fn text_only_accepts_static_text_without_new_expressions() {
        let rule = Rule::TextOnly;
        let reworded = SUMMARIZE.replace(
            "Summarize this document:",
            "Summarize this document in three bullet points:",
        );
        assert_eq!(run(&rule, SUMMARIZE, &reworded), Ok(()));
        let reformatted = SUMMARIZE.replace("${doc}", "${ doc }");
        assert_eq!(run(&rule, SUMMARIZE, &reformatted), Ok(()));
        let added = SUMMARIZE.replace("${doc}`", "${doc}\\n\\nRelated: ${lookup(doc)}`");
        assert_eq!(
            run(&rule, SUMMARIZE, &added),
            Err("the change adds an expression, `${lookup(doc)}`".to_string())
        );
        let client = SUMMARIZE.replace("client: Writer", "client: Other");
        assert_eq!(
            run(&rule, SUMMARIZE, &client),
            Err("the change edits `client`".to_string())
        );
        let signature = SUMMARIZE.replace("-> Summary", "-> string");
        assert!(run(&rule, SUMMARIZE, &signature).is_err());

        // Any function's strings and templates, not only a prompt.
        let greeting = "/// baml:hotswap=hotswap.text_only\nfunction Greet(n: int) -> string {\n  log.info(\"greeting\");\n  `hello #${n}`\n}";
        let edited = greeting
            .replace("\"greeting\"", "\"greeting someone\"")
            .replace("`hello #${n}`", "`hi there, number ${n}!`");
        assert_eq!(run(&rule, greeting, &edited), Ok(()));
        let code = greeting.replace("${n}`", "${n + 1}`");
        assert_eq!(
            run(&rule, greeting, &code),
            Err("the change adds an expression, `${n + 1}`".to_string())
        );
        let call = greeting.replace("log.info(\"greeting\");\n", "");
        assert_eq!(
            run(&rule, greeting, &call),
            Err("the change edits more than static text".to_string())
        );
        assert!(run(&rule, TRIAGE, TRIAGE).is_ok(), "no change at all");
    }

    const REPLY: &str = "/// baml:hotswap=hotswap.no_new_calls\nfunction Reply(ticket: Ticket) -> string {\n  let customer = LookupCustomer(ticket.customer_id)\n  DraftReply(ticket, customer)\n}";

    #[test]
    fn no_new_calls_rejects_a_new_callee() {
        let rule = Rule::NoNewCalls;
        let adjusted = REPLY.replace(
            "DraftReply(ticket, customer)",
            "DraftReply(ticket, customer, tone = \"formal\")",
        );
        assert_eq!(run(&rule, REPLY, &adjusted), Ok(()));
        let refund = REPLY.replace(
            "  DraftReply(",
            "  RefundOrder(ticket.order_id)\n  DraftReply(",
        );
        assert_eq!(
            run(&rule, REPLY, &refund),
            Err("the replacement calls `RefundOrder`, which the original does not".to_string())
        );
        let method = REPLY.replace("LookupCustomer(", "customers.delete(");
        assert!(run(&rule, REPLY, &method).is_err());
        let client = SUMMARIZE.replace("client: Writer", "client: Other");
        assert!(run(&rule, SUMMARIZE, &client).is_err());
    }

    #[test]
    fn any_accepts_everything_and_only_any_accepts_a_client_change() {
        let edited = REPLY.replace("DraftReply", "Other");
        assert_eq!(run(&Rule::Any, REPLY, &edited), Ok(()));
        let clients = ["Writer".to_string()];
        let change = Change {
            original: SUMMARIZE,
            replacement: SUMMARIZE,
            changed_clients: &clients,
        };
        assert_eq!(check(&Rule::Any, &change, &|_| None), Ok(()));
        assert_eq!(
            check(&Rule::TextOnly, &change, &|_| None),
            Err("client `Writer` changed, and only hotswap.any accepts a client change".into())
        );
    }
}
