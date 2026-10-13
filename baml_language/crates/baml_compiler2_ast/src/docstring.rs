//! Docstring extraction from CST trivia.
//!
//! `///`-prefixed line comments preceding an item declaration are collected
//! during CST → AST lowering and stored on the corresponding AST node.

use baml_compiler_syntax::{SyntaxKind, SyntaxNode};

/// How a `//baml:<marker>` directive line begins. A directive belongs to the
/// declaration it precedes, like an attribute: it is not a comment.
const DIRECTIVE_PREFIX: &str = "//baml:";

fn trace_directive_value(text: &str) -> Option<&str> {
    text.trim_start_matches('/')
        .trim()
        .strip_prefix("baml:$trace")
}

/// Reject directives outside the leading comments of a function definition.
/// Scan comment tokens so fields, signatures, and unattached comments cannot
/// silently discard a directive either. Text inside strings is not a directive.
pub(crate) fn reject_unsupported_trace_hooks(
    root: &SyntaxNode,
    diagnostics: &mut Vec<crate::LoweringDiagnostic>,
) {
    for token in root
        .descendants_with_tokens()
        .filter_map(baml_compiler_syntax::NodeOrToken::into_token)
    {
        if token.kind() != SyntaxKind::LINE_COMMENT || trace_directive_value(token.text()).is_none()
        {
            continue;
        }
        let attached_to_function = token.parent().is_some_and(|parent| {
            parent.kind() == SyntaxKind::FUNCTION_DEF
                && parent.children_with_tokens()
                    .take_while(|child| matches!(child,
                        rowan::NodeOrToken::Token(token) if matches!(token.kind(),
                            SyntaxKind::LINE_COMMENT | SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE
                        )
                    ))
                    .any(|child| child.as_token() == Some(&token))
        });
        if !attached_to_function {
            diagnostics.push(crate::LoweringDiagnostic::InvalidTraceHook {
                message: "Trace hooks can only be attached to function definitions.\nhelp: Place `/// baml:$trace=...` immediately before a function definition.".into(),
                span: token.text_range(),
            });
        }
    }
}

/// Extract `///` doc comments attached to `node`.
///
/// The BAML parser captures leading line comments as the *first children*
/// of the item node they precede (rather than as siblings before it), so
/// this walks `node.children_with_tokens()` from the start. Logic mirrors
/// what readers expect: only the contiguous run of `///` lines *immediately
/// before the declaration body* counts as its docstring. A non-doc `// …`
/// line interleaved among the leading comments resets the accumulator —
/// e.g. file-header `// …` blocks separated by a blank line from a `///`
/// block don't pollute the docstring, and a stray `// …` after the `///`
/// block detaches the docstring entirely. A `//baml:` directive is not such
/// a line: it is part of the declaration, so a doc written above one stays
/// attached (every documented stdlib native reads `/// …`, then
/// `//baml:mut_self`, then `function …`). The walk stops at the first
/// non-trivia token or child node.
///
/// Returns `None` when no `///` lines are immediately attached; otherwise
/// returns the joined lines (one `\n` between originals, with a single
/// optional leading space stripped from each `///` body).
pub fn extract_docstring(node: &SyntaxNode) -> Option<String> {
    let mut doc_lines: Vec<String> = Vec::new();

    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Token(tok) => match tok.kind() {
                SyntaxKind::LINE_COMMENT => {
                    let text = tok.text();
                    if let Some(doc) = text.strip_prefix("///") {
                        let doc = doc.strip_prefix(' ').unwrap_or(doc);
                        if !doc.trim_start().starts_with("baml:$trace") {
                            doc_lines.push(doc.to_string());
                        }
                    } else if !text.starts_with(DIRECTIVE_PREFIX) {
                        // Regular `// …` line interleaved with leading
                        // trivia detaches any earlier `///` accumulation
                        // from the declaration. A directive is part of the
                        // declaration, so it leaves the doc attached.
                        doc_lines.clear();
                    }
                }
                SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE => {}
                _ => break,
            },
            rowan::NodeOrToken::Node(_) => break,
        }
    }

    if doc_lines.is_empty() {
        return None;
    }

    Some(doc_lines.join("\n"))
}

/// Return `true` when a `//baml:<marker>` directive appears in the leading
/// comment trivia of `node`. Walks the same children prefix as
/// `extract_docstring` so the same "immediately attached" semantics apply.
///
/// Used by BEP-049 §10 to detect `//baml:tagged_string` on a function
/// definition; can be reused for any future single-keyword directive.
pub fn has_baml_marker(node: &SyntaxNode, marker: &str) -> bool {
    let needle = format!("{DIRECTIVE_PREFIX}{marker}");
    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Token(tok) => match tok.kind() {
                SyntaxKind::LINE_COMMENT => {
                    if tok.text().trim_end() == needle {
                        return true;
                    }
                }
                SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE => {}
                _ => return false,
            },
            rowan::NodeOrToken::Node(_) => return false,
        }
    }
    false
}

/// Extract the single declaration hook. Directives stay out of user docs.
pub fn trace_hook(
    node: &SyntaxNode,
    function_name: &str,
    diagnostics: &mut Vec<crate::LoweringDiagnostic>,
) -> Option<crate::ast::TraceHookDirective> {
    let mut hook = None;
    for child in node.children_with_tokens() {
        let rowan::NodeOrToken::Token(token) = child else {
            break;
        };
        match token.kind() {
            SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE => continue,
            SyntaxKind::LINE_COMMENT => {}
            _ => break,
        }
        let Some(value) = trace_directive_value(token.text()) else {
            continue;
        };
        let path = value.trim_start().strip_prefix('=').map(str::trim);
        let valid = path.is_some_and(|path| {
            !path.is_empty()
                && path.split('.').all(|segment| {
                    let mut chars = segment.chars();
                    chars
                        .next()
                        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
                        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
                })
        });
        if !valid || hook.is_some() {
            diagnostics.push(crate::LoweringDiagnostic::InvalidTraceHook {
                message: if hook.is_some() {
                    format!("Function `{function_name}` declares more than one trace hook.\nhelp: Keep one `/// baml:$trace=...` directive.")
                } else {
                    format!("Invalid trace hook directive on function `{function_name}`.\nExpected `/// baml:$trace=hook_name` with a function reference.\nhelp: Use a function name without call parentheses, for example `/// baml:$trace=trace.empty_span`.")
                },
                span: token.text_range(),
            });
            continue;
        }
        hook = Some(crate::ast::TraceHookDirective {
            path: path.unwrap().split('.').map(baml_base::Name::new).collect(),
            span: token.text_range(),
        });
    }
    hook
}
