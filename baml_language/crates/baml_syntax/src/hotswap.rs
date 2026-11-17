//! The `/// baml:hotswap` mark (BEP-087) and the built-in rules it can name.
//! The parser reads the mark; `bex_project::hotswap_rules` runs the rules.

/// The rule a mark names, or the project's `default_rule`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    /// `hotswap.select_only([A, B])`: the replacement's code must match one
    /// of the shipped functions named, compared by code, not by name.
    SelectOnly(Vec<String>),
    /// `hotswap.telemetry_only`: only log lines, `$trace` arguments and
    /// `$trace` hooks may change.
    TelemetryOnly,
    /// `hotswap.text_only`: only the static text of string and template
    /// literals, prompts included, with no new `${...}` expression.
    TextOnly,
    /// `hotswap.no_new_calls`: nothing called that the original did not call.
    NoNewCalls,
    /// `hotswap.any`: anything that keeps the public interface.
    Any,
}

impl std::fmt::Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Rule::SelectOnly(names) => write!(f, "hotswap.select_only([{}])", names.join(", ")),
            Rule::TelemetryOnly => f.write_str("hotswap.telemetry_only"),
            Rule::TextOnly => f.write_str("hotswap.text_only"),
            Rule::NoNewCalls => f.write_str("hotswap.no_new_calls"),
            Rule::Any => f.write_str("hotswap.any"),
        }
    }
}

impl Rule {
    /// The rule's name without its arguments, as the CLI reports it.
    pub fn name(&self) -> &'static str {
        match self {
            Rule::SelectOnly(_) => "hotswap.select_only",
            Rule::TelemetryOnly => "hotswap.telemetry_only",
            Rule::TextOnly => "hotswap.text_only",
            Rule::NoNewCalls => "hotswap.no_new_calls",
            Rule::Any => "hotswap.any",
        }
    }

    /// Parse a rule as a mark or `default_rule` spells it. Stage 2 has only
    /// the built-in rules. `select_only` takes a bracketed list of function
    /// names, as the BEP illustrates it.
    pub fn parse(text: &str) -> Result<Rule, String> {
        let text = text.trim();
        if let Some(rest) = text.strip_prefix("hotswap.select_only") {
            let list = rest
                .trim()
                .strip_prefix('(')
                .and_then(|rest| rest.strip_suffix(')'))
                .map(str::trim)
                .and_then(|rest| rest.strip_prefix('['))
                .and_then(|rest| rest.strip_suffix(']'))
                .ok_or_else(|| {
                    format!("`{text}`: `hotswap.select_only` takes a list, such as `hotswap.select_only([A, B])`")
                })?;
            let names: Vec<String> = list
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect();
            if names.is_empty() {
                return Err(format!(
                    "`{text}`: `hotswap.select_only` needs at least one function"
                ));
            }
            if let Some(bad) = names.iter().find(|name| {
                !name
                    .split('.')
                    .all(crate::lexer::is_identifier)
            }) {
                return Err(format!("`{text}`: `{bad}` is not a function name"));
            }
            return Ok(Rule::SelectOnly(names));
        }
        match text {
            "hotswap.telemetry_only" => Ok(Rule::TelemetryOnly),
            "hotswap.text_only" => Ok(Rule::TextOnly),
            "hotswap.no_new_calls" => Ok(Rule::NoNewCalls),
            "hotswap.any" => Ok(Rule::Any),
            _ => Err(format!(
                "`{text}` is not a built-in rule: use hotswap.select_only, hotswap.telemetry_only, hotswap.text_only, hotswap.no_new_calls or hotswap.any"
            )),
        }
    }
}

/// What a `/// baml:hotswap` mark says: the rule it names, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub rule: Option<Rule>,
}

/// The mark in one comment's text, `/// baml:hotswap` or `//baml:hotswap`,
/// optionally `=RULE`: `None` when the comment is no hotswap mark, and an
/// error when it names no built-in rule.
pub fn parse_mark(comment: &str) -> Option<Result<Mark, String>> {
    let body = comment.trim_start_matches('/').trim();
    let rest = body.strip_prefix("baml:hotswap")?.trim_end();
    if rest.is_empty() {
        return Some(Ok(Mark { rule: None }));
    }
    let rule = rest.strip_prefix('=')?;
    Some(Rule::parse(rule).map(|rule| Mark { rule: Some(rule) }))
}
