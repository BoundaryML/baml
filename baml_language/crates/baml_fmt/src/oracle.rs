//! What every formatted output must satisfy against the input it came from,
//! whatever the rules did: it parses without errors; it has the same nodes
//! with the same tokens, so only whitespace and comment placement changed;
//! and it has every comment, each exactly once. The formatter checks it in
//! debug builds, and the tests check it on every case and corpus file.
//!
//! "The same" allows exactly what the rules canonicalise: a member's or
//! statement's optional separator (`,`, `;`, a field's `:`), a list's
//! separators, `=>` for `->`, `implement` for `implements`, the order of an
//! LLM body's fields, a stray `;`, the parentheses around a loop condition
//! or `for` header, and the layout of a multi-line backtick string (its
//! dedented text is what is compared).

use std::fmt;

use baml_base::dedent::dedent_backtick;
use baml_parser::ParseError;
use baml_syntax::{
    nodes::{AnyId, AnyTemplatePart, FileTree},
    token::{TokenIdx, TokenKind},
    tree::{AnyNodeId, Missing, NodeUnion as _, TokenRange, TreeWalk as _, Visitor},
};

/// How a formatted output fails the oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// The output does not parse.
    ParseErrors(Vec<ParseError>),
    /// The output's tree differs from the input's: the first differing entry
    /// of each shape.
    Shape { expected: String, found: String },
    /// A comment was lost or duplicated.
    Comments {
        expected: Vec<String>,
        found: Vec<String>,
    },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParseErrors(errors) => write!(f, "the output does not parse: {errors:?}"),
            Self::Shape { expected, found } => {
                write!(
                    f,
                    "the output's tree differs: expected {expected:?}, found {found:?}"
                )
            }
            Self::Comments { expected, found } => {
                write!(
                    f,
                    "the output's comments differ: expected {expected:?}, found {found:?}"
                )
            }
        }
    }
}

impl std::error::Error for Violation {}

/// Check `output`, the formatted text of `input`.
pub fn check(input: &FileTree, output: &str) -> Result<(), Violation> {
    let parsed = baml_parser::parse(output);
    if !parsed.errors.is_empty() {
        return Err(Violation::ParseErrors(parsed.errors));
    }
    let expected = shape(input);
    let found = shape(&parsed.tree);
    let end = || "<end>".to_owned();
    let first_difference =
        (0..expected.len().max(found.len())).find_map(|i| match (expected.get(i), found.get(i)) {
            (Some(expected), Some(found)) if expected == found => None,
            (expected, found) => Some((
                expected.cloned().unwrap_or_else(end),
                found.cloned().unwrap_or_else(end),
            )),
        });
    if let Some((expected, found)) = first_difference {
        return Err(Violation::Shape { expected, found });
    }
    let expected = comments(input);
    let found = comments(&parsed.tree);
    if expected != found {
        return Err(Violation::Comments { expected, found });
    }
    Ok(())
}

/// Every node in source order, each as its kind and, in field order, its
/// tokens' texts and the kinds of its children — minus what the rules
/// canonicalise (see the module doc).
fn shape(tree: &FileTree) -> Vec<String> {
    struct Parts<'t> {
        tree: &'t FileTree,
        out: String,
        children: Vec<AnyNodeId>,
    }
    impl Visitor for Parts<'_> {
        fn node(&mut self, _field: &'static str, id: AnyNodeId) {
            let Some(id) = seen_through(self.tree, id) else {
                return;
            };
            self.out.push_str(" (");
            self.out.push_str(self.tree.arenas().kind_name(id));
            self.out.push(')');
            self.children.push(id);
        }
        fn token(&mut self, field: &'static str, token: TokenIdx) {
            let text = match (field, self.tree.token_text(token)) {
                // Optional punctuation the rules add or replace.
                ("sep" | "semi" | "comma" | "colon", _) => return,
                ("arrow", "=>") => "->",
                ("kw", "implement") => "implements",
                // Compared dedented, by the template as a whole.
                (_, _) if self.tree.tokens().kind(token) == TokenKind::TemplateChunk => return,
                (_, text) => text,
            };
            self.out.push(' ');
            self.out.push_str(text);
        }
        fn separator(&mut self, _field: &'static str, _token: TokenIdx) {}
        fn range(&mut self, _field: &'static str, range: TokenRange) {
            for token in range.iter() {
                if !self.tree.tokens().kind(token).is_trivia() {
                    self.out.push(' ');
                    self.out.push_str(self.tree.token_text(token));
                }
            }
        }
        fn missing(&mut self, _field: &'static str, _missing: Missing) {
            self.out.push_str(" <missing>");
        }
    }
    let mut lines = Vec::new();
    let mut stack = vec![tree.root_id()];
    while let Some(id) = stack.pop() {
        let kind = tree.arenas().kind_name(id);
        let mut parts = Parts {
            tree,
            out: kind.to_owned(),
            children: Vec::new(),
        };
        tree.arenas().walk_node(id, &mut parts);
        let mut children = parts.children;
        match id.typed() {
            AnyId::LlmBody(_) => {
                // The fields print in a fixed order whatever the source's.
                children.sort_by_key(|&child| tree.arenas().kind_name(child));
                kind.clone_into(&mut parts.out);
            }
            AnyId::BacktickString(template) => {
                // The text with every non-text part held out, dedented: what
                // the literal means, whatever its layout.
                let joined: String = tree
                    .list(tree.get(template).parts)
                    .iter()
                    .map(|part| match *part {
                        AnyTemplatePart::Chunk(chunk) => tree.token_text(chunk.idx),
                        AnyTemplatePart::Interp(_)
                        | AnyTemplatePart::For(_)
                        | AnyTemplatePart::If(_)
                        | AnyTemplatePart::Bogus(_) => "\0",
                    })
                    .collect();
                parts.out.push(' ');
                parts.out.push_str(&dedent_backtick(&joined));
            }
            _ => {}
        }
        lines.push(parts.out);
        stack.extend(children.into_iter().rev());
    }
    lines
}

/// The node `id` stands for in the compared shape: none for a stray `;`,
/// which the rules drop; the inner node for parentheses the rules add
/// around a loop condition or `for` header, however many; else itself.
fn seen_through(tree: &FileTree, id: AnyNodeId) -> Option<AnyNodeId> {
    match id.typed() {
        AnyId::EmptyStmt(_) => None,
        AnyId::ParenExpr(paren) => {
            let inner = tree.get(paren).inner;
            seen_through(tree, inner.ok()?.node_id())
        }
        AnyId::ParenForInHeader(paren) => Some(AnyNodeId::of(tree.get(paren).header)),
        _ => Some(id),
    }
}

/// The text of every plain comment, sorted.
fn comments(tree: &FileTree) -> Vec<String> {
    let tokens = tree.tokens();
    let mut texts: Vec<String> = tokens
        .iter()
        .filter(|&token| {
            matches!(
                tokens.kind(token),
                TokenKind::LineComment
                    | TokenKind::HeaderComment
                    | TokenKind::BlockComment
                    | TokenKind::UnclosedBlockComment
            )
        })
        .map(|token| tree.token_text(token).trim_end().to_owned())
        .collect();
    texts.sort();
    texts
}
