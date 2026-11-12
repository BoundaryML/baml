//! What every formatted output must satisfy against the input it came from,
//! whatever the rules did: it parses without errors; it has the same nodes
//! with the same tokens, so only whitespace and comment placement changed;
//! and it has every comment, each exactly once. The formatter checks it in
//! debug builds, and the tests check it on every case and corpus file.

use std::fmt;

use baml_parser::ParseError;
use baml_syntax::{
    index,
    nodes::FileTree,
    token::{TokenIdx, TokenKind},
    tree::{AnyNodeId, Missing, TokenRange, TreeWalk as _, Visitor},
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
/// tokens (kind and text) and the kinds of its children.
fn shape(tree: &FileTree) -> Vec<String> {
    struct Parts<'t> {
        tree: &'t FileTree,
        out: String,
    }
    impl Visitor for Parts<'_> {
        fn node(&mut self, _field: &'static str, id: AnyNodeId) {
            self.out.push_str(" (");
            self.out.push_str(self.tree.arenas().kind_name(id));
            self.out.push(')');
        }
        fn token(&mut self, _field: &'static str, token: TokenIdx) {
            self.out.push(' ');
            self.out.push_str(self.tree.token_text(token));
        }
        fn range(&mut self, _field: &'static str, range: TokenRange) {
            for token in range.iter() {
                if !self.tree.tokens().kind(token).is_trivia() {
                    self.token("", token);
                }
            }
        }
        fn missing(&mut self, _field: &'static str, _missing: Missing) {
            self.out.push_str(" <missing>");
        }
    }
    index::descendants(tree, tree.root_id())
        .map(|id| {
            let mut parts = Parts {
                tree,
                out: tree.arenas().kind_name(id).to_owned(),
            };
            tree.arenas().walk_node(id, &mut parts);
            parts.out
        })
        .collect()
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
