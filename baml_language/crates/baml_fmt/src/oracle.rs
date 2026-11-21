//! What every formatted output must satisfy against the input it came from,
//! whatever the rules did: it parses without errors; it has the same
//! comments, so none was lost or duplicated; it has the same nodes with the
//! same tokens, so only whitespace and comment placement changed; and every
//! container — a file, a block, a body — holds the same comments, so none
//! left the list of items, statements or members it was written in (within
//! one, the rules do move comments; see `shape`). The formatter checks it
//! on every run and refuses to hand out an output that fails it
//! ([`FormatterError::Violation`](crate::FormatterError::Violation)), and
//! the tests check it on every case and corpus file.
//!
//! "The same" allows exactly what the rules canonicalise: a member's or
//! statement's optional separator (`,`, `;`, a field's `:`), a list's
//! separators, `=>` for `->`, `implement` for `implements`, the order of an
//! LLM body's fields (whose comments are compared as one set, since the
//! fields move with theirs and a comment closing the body stays at its end
//! whichever field was written last), a stray `;`, redundant parentheses
//! around an expression — but not around a call's callee or a generic
//! application's base, nor around a receiver that ends an optional chain
//! (`(a?.b).c` reads `.c` on `null` where `a?.b.c` skips it), which the
//! rules keep — the `;` that closes a diverging tail (`{ throw x }` →
//! `{ throw x; }`), trailing blanks on a `///`, `//baml:` or `//#` line,
//! and the layout of a multi-line backtick string (its dedented text is
//! what is compared; the text inside its `${for}` and `${if}` blocks, which
//! the rules print as written, is compared exactly).

use std::fmt;

use baml_base::dedent::dedent_backtick;
use baml_parser::ParseError;
use baml_syntax::{
    index::TreeIndex,
    nodes::{AnyExpr, AnyId, AnyTemplatePart, FileTree},
    token::{TokenIdx, TokenKind},
    tree::{AnyNodeId, Missing, NodeUnion as _, TokenRange, TreeWalk as _, Visitor},
};
use rustc_hash::FxHashMap;

use crate::{
    comments::{Comments, Position, is_comment},
    format::{diverges, ends_optional_chain},
};

/// How a formatted output fails the oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// The output does not parse.
    ParseErrors(Vec<ParseError>),
    /// The output's comments differ from the input's: those of the input
    /// the output lacks, and those of the output the input lacks.
    Comments {
        missing: Vec<String>,
        added: Vec<String>,
    },
    /// The output's tree differs from the input's, or a container's comments
    /// do: the first differing entry of each shape.
    Shape { expected: String, found: String },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParseErrors(errors) => write!(f, "the output does not parse: {errors:?}"),
            Self::Comments { missing, added } => write!(
                f,
                "the output's comments differ: missing {}, added {}",
                a_few(missing),
                a_few(added)
            ),
            Self::Shape { expected, found } => {
                // A container's entry lists every comment inside it: the
                // message shows the two entries around where they differ.
                let at = expected
                    .bytes()
                    .zip(found.bytes())
                    .position(|(expected, found)| expected != found)
                    .unwrap_or(expected.len().min(found.len()));
                write!(
                    f,
                    "the output's tree differs: expected {}, found {}",
                    excerpt(expected, at),
                    excerpt(found, at)
                )
            }
        }
    }
}

impl std::error::Error for Violation {}

/// Up to three of `texts`, each cut short, and how many more there are.
fn a_few(texts: &[String]) -> String {
    const SHOWN: usize = 3;
    const WIDTH: usize = 60;
    let shown: Vec<String> = texts
        .iter()
        .take(SHOWN)
        .map(|text| match text.char_indices().nth(WIDTH) {
            Some((at, _)) => format!("{}…", &text[..at]),
            None => text.clone(),
        })
        .collect();
    match texts.len().checked_sub(SHOWN) {
        Some(more) if more > 0 => format!("{shown:?} and {more} more"),
        _ => format!("{shown:?}"),
    }
}

/// `text` around byte offset `at`, quoted, with what is left out marked.
fn excerpt(text: &str, at: usize) -> String {
    const BEFORE: usize = 60;
    const AFTER: usize = 160;
    let mut start = at.saturating_sub(BEFORE).min(text.len());
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = at.saturating_add(AFTER).min(text.len());
    while !text.is_char_boundary(end) {
        end += 1;
    }
    let head = if start > 0 { "…" } else { "" };
    let tail = if end < text.len() { "…" } else { "" };
    format!("{head}{:?}{tail}", &text[start..end])
}

/// Check `output`, the formatted text of `input`.
pub fn check(input: &FileTree, output: &str) -> Result<(), Violation> {
    let parsed = baml_parser::parse(output);
    if !parsed.errors.is_empty() {
        return Err(Violation::ParseErrors(parsed.errors));
    }
    let expected_comments = comment_texts(input);
    let found_comments = comment_texts(&parsed.tree);
    if expected_comments != found_comments {
        return Err(Violation::Comments {
            missing: multiset_difference(&expected_comments, &found_comments),
            added: multiset_difference(&found_comments, &expected_comments),
        });
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
    match first_difference {
        Some((expected, found)) => Err(Violation::Shape { expected, found }),
        None => Ok(()),
    }
}

/// The text of every comment token of `tree`, sorted, with the trailing
/// blanks the printer drops. Comments the rules print as written — inside a
/// removed item's body, a jinja argument — count the same as the ones the
/// comment layer places.
fn comment_texts(tree: &FileTree) -> Vec<String> {
    let tokens = tree.tokens();
    let mut texts: Vec<String> = tokens
        .iter()
        .filter(|&token| is_comment(tokens.kind(token)))
        .map(|token| tree.token_text(token).trim_end().to_owned())
        .collect();
    texts.sort();
    texts
}

/// The elements of the sorted multiset `from` not in the sorted multiset
/// `minus`, each counted as often as it is in `from` beyond `minus`.
fn multiset_difference(from: &[String], minus: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut minus = minus.iter().peekable();
    for text in from {
        while minus.next_if(|&other| other < text).is_some() {}
        if minus.next_if(|&other| other == text).is_none() {
            out.push(text.clone());
        }
    }
    out
}

/// Every node in source order, each as its kind and, in field order, its
/// tokens' texts and the kinds of its children — minus what the rules
/// canonicalise (see the module doc) — and, on each container's entry, the
/// comments attached anywhere inside it, in source order. A container holds
/// a list of items, statements or members: a file, a block, a body, an LLM
/// body (whose comments are one sorted set, as its fields are reordered).
/// The rules move a comment between the nodes inside one container (a
/// comment trailing a name tucks after the `(` that follows it, one before
/// an added `,` dangles after it and so leads the next item on re-reading)
/// but never across containers, never out of order, and never lose or
/// duplicate one.
fn shape(tree: &FileTree) -> Vec<String> {
    struct Parts<'t> {
        tree: &'t FileTree,
        /// The node whose parts these are.
        parent: AnyNodeId,
        out: String,
        children: Vec<AnyNodeId>,
        /// Nodes read through (parentheses the rules drop, a stray `;`, a
        /// member spine's inner accesses): their comments still count.
        skipped: Vec<AnyNodeId>,
    }
    impl Visitor for Parts<'_> {
        fn node(&mut self, field: &'static str, id: AnyNodeId) {
            // Parentheses around a callee (`(v.f)(3)` calls the field's
            // value, `v.f(3)` the method) or a generic application's base
            // are kept by the rules, and so are read as they are; so are
            // those around a receiver that end an optional chain.
            let kept = match id.typed() {
                AnyId::ParenExpr(paren) => match self.parent.typed() {
                    AnyId::CallExpr(_) | AnyId::OptionalCallExpr(_) => field == "callee",
                    AnyId::GenericApplyExpr(_) => field == "base",
                    _ => {
                        receiver_field(self.parent, field)
                            && ends_optional_chain(self.tree, AnyExpr::Paren(paren))
                    }
                },
                _ => false,
            };
            let id = if kept {
                Some(id)
            } else {
                seen_through(self.tree, id, &mut self.skipped)
            };
            let Some(id) = id else {
                return;
            };
            self.out.push_str(" (");
            self.out.push_str(kind_label(self.tree, id));
            self.out.push(')');
            self.children.push(id);
        }
        fn token(&mut self, field: &'static str, token: TokenIdx) {
            let kind = self.tree.tokens().kind(token);
            let text = match (field, self.tree.token_text(token)) {
                // Optional punctuation the rules add or replace.
                ("sep" | "semi" | "comma" | "colon", _) => return,
                ("arrow", "=>") => "->",
                ("kw", "implement") => "implements",
                // A template's own chunks are compared dedented, by the
                // template as a whole; those inside its blocks as written.
                (_, _)
                    if kind == TokenKind::TemplateChunk
                        && matches!(self.parent.typed(), AnyId::BacktickString(_)) =>
                {
                    return;
                }
                // The rules drop a doc, directive or header line's trailing
                // blanks.
                (_, text)
                    if matches!(
                        kind,
                        TokenKind::HeaderComment
                            | TokenKind::DocComment
                            | TokenKind::DirectiveComment
                    ) =>
                {
                    text.trim_end()
                }
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
    /// Whether `id` is a container: its children's comments and their
    /// descendants' are compared as one sequence. Exactly the nodes whose
    /// children end in a terminator: a comment carried past a statement's
    /// `;` (a type suffix's) is the statement's own on re-reading, or the
    /// next statement's when it ends up on a line of its own, so neither a
    /// statement nor a top-level item is a container of its own — their
    /// insides are compared with their siblings'. An LLM body is one too,
    /// its comments compared as a set, since its fields are reordered.
    fn holds_scopes(id: AnyNodeId) -> bool {
        matches!(
            id.typed(),
            AnyId::SourceFile(_)
                | AnyId::Block(_)
                | AnyId::ClassDef(_)
                | AnyId::EnumDef(_)
                | AnyId::InterfaceDef(_)
                | AnyId::ImplementsBlock(_)
                | AnyId::ImplementsFor(_)
                | AnyId::LlmBody(_)
        )
    }
    let index = TreeIndex::build(tree);
    let comments = Comments::from_tree(tree, &index);
    let mut lines: Vec<String> = Vec::new();
    let mut line_of: FxHashMap<AnyNodeId, usize> = FxHashMap::default();
    let mut scope_comments: FxHashMap<AnyNodeId, Vec<(TokenIdx, String)>> = FxHashMap::default();
    // A node's comments count in the container it is in — except those
    // dangling inside a container, which are that container's: a comment in
    // an empty block is the block's whether it dangles after the `{` or
    // after a stray `;` the rules drop.
    let note = |scope: AnyNodeId,
                id: AnyNodeId,
                scope_comments: &mut FxHashMap<_, Vec<(TokenIdx, String)>>| {
        for comment in comments.of(id) {
            let inside = holds_scopes(id) && matches!(comment.position, Position::Dangling { .. });
            let target = if inside { id } else { scope };
            scope_comments
                .entry(target)
                .or_default()
                .push((comment.token, comment.text(tree).trim_end().to_owned()));
        }
    };
    let root = tree.root_id();
    // Each node with the container it is in: the nearest container above it.
    let mut stack: Vec<(AnyNodeId, AnyNodeId)> = vec![(root, root)];
    while let Some((id, scope)) = stack.pop() {
        line_of.insert(id, lines.len());
        note(scope, id, &mut scope_comments);
        let kind = tree.arenas().kind_name(id);
        let mut parts = Parts {
            tree,
            parent: id,
            out: kind.to_owned(),
            children: Vec::new(),
            skipped: Vec::new(),
        };
        let child_scope = |_child: AnyNodeId| if holds_scopes(id) { id } else { scope };
        if let Some(spine) = member_spine(tree, id) {
            // `(xs).join()` reparses as the path `xs.join()`: a member access
            // on a path or on another member access is one spine, whatever
            // nodes spell it.
            lines.push(spine.text);
            // The head was noted above with every popped node.
            for inner in spine.nodes.into_iter().filter(|&inner| inner != id) {
                note(scope, inner, &mut scope_comments);
            }
            stack.extend(spine.base.map(|base| (base, child_scope(base))));
            continue;
        }
        tree.arenas().walk_node(id, &mut parts);
        // Nodes read through are children of this one: they are in the
        // container its children are in.
        for skipped in parts.skipped {
            note(child_scope(skipped), skipped, &mut scope_comments);
        }
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
        stack.extend(
            children
                .into_iter()
                .rev()
                .map(|child| (child, child_scope(child))),
        );
    }
    for (scope, mut texts) in scope_comments {
        // Source order — except in an LLM body, whose fields move.
        if matches!(scope.typed(), AnyId::LlmBody(_)) {
            texts.sort_by(|a, b| a.1.cmp(&b.1));
        } else {
            texts.sort_by_key(|&(token, _)| token);
        }
        let line = &mut lines[line_of[&scope]];
        for (_, text) in texts {
            line.push_str(" #");
            line.push_str(&text);
        }
    }
    lines
}

/// Whether `field` of `parent` is the receiver of a member access, index,
/// upcast, `@spec` or tagged template: where parentheses that end an
/// optional chain stay.
fn receiver_field(parent: AnyNodeId, field: &str) -> bool {
    match parent.typed() {
        AnyId::FieldAccessExpr(_)
        | AnyId::OptionalFieldAccessExpr(_)
        | AnyId::IndexExpr(_)
        | AnyId::OptionalIndexExpr(_)
        | AnyId::TupleFieldExpr(_)
        | AnyId::UpcastExpr(_)
        | AnyId::SpecExpr(_) => field == "base",
        AnyId::TaggedTemplateExpr(_) => field == "tag",
        _ => false,
    }
}

/// How a child is named in its parent's entry: a path and a member access
/// are both a `Member` spine (see [`member_spine`]).
fn kind_label(tree: &FileTree, id: AnyNodeId) -> &'static str {
    match id.typed() {
        AnyId::PathExpr(_) | AnyId::FieldAccessExpr(_) => "Member",
        _ => tree.arenas().kind_name(id),
    }
}

/// A member-access spine `base.a.b`, as one entry: the names it accesses,
/// and the base they are accessed on when that is not a plain name.
struct Spine {
    text: String,
    base: Option<AnyNodeId>,
    /// The access nodes merged into the entry, the names included: their
    /// comments count in the container's.
    nodes: Vec<AnyNodeId>,
}

/// The spine `id` heads, if it is a `PathExpr` or a `FieldAccessExpr`: the
/// two spell the same access (`xs.join` is a path, `(xs).join` a member
/// access on a parenthesised path), so both read as one `Member` entry
/// over the innermost base that is neither.
fn member_spine(tree: &FileTree, id: AnyNodeId) -> Option<Spine> {
    let mut names: Vec<String> = Vec::new();
    let mut nodes: Vec<AnyNodeId> = Vec::new();
    let mut at = id;
    let base = loop {
        nodes.push(at);
        match at.typed() {
            AnyId::PathExpr(path) => {
                let node = tree.get(path);
                for segment in tree.list(node.segments).iter().rev() {
                    let name = segment.name.ok();
                    names.push(format!(
                        "{}{}",
                        tree.token_text(segment.dot.idx),
                        name.map_or("<missing>", |name| tree
                            .token_text(tree.get(name).token.idx))
                    ));
                    nodes.extend(name.map(AnyNodeId::of));
                }
                names.push(tree.token_text(tree.get(node.head).token.idx).to_owned());
                nodes.push(AnyNodeId::of(node.head));
                break None;
            }
            AnyId::FieldAccessExpr(access) => {
                let node = tree.get(access);
                let name = node.name.ok();
                names.push(format!(
                    "{}{}",
                    tree.token_text(node.dot.idx),
                    name.map_or("<missing>", |name| tree
                        .token_text(tree.get(name).token.idx))
                ));
                nodes.extend(name.map(AnyNodeId::of));
                // Parentheses that end an optional chain are kept, and so
                // are a base of their own.
                if matches!(node.base, AnyExpr::Paren(_)) && ends_optional_chain(tree, node.base) {
                    break Some(node.base.node_id());
                }
                let base = seen_through(tree, node.base.node_id(), &mut nodes)?;
                match base.typed() {
                    AnyId::PathExpr(_) | AnyId::FieldAccessExpr(_) => at = base,
                    _ => break Some(base),
                }
            }
            _ => return None,
        }
    };
    names.reverse();
    Some(Spine {
        text: format!("Member {}", names.join(" ")),
        base,
        nodes,
    })
}

/// The node `id` stands for in the compared shape: none for a stray `;`,
/// which the rules drop; the inner node for parentheses the rules add
/// around a loop or `if` condition, a `match` scrutinee or a `for` header,
/// however many; the expression of a diverging expression statement, which
/// the rules make of a diverging tail (`{ throw x }` → `{ throw x; }`);
/// else itself. Every node read through goes on `skipped`: its comments
/// still count.
fn seen_through(tree: &FileTree, id: AnyNodeId, skipped: &mut Vec<AnyNodeId>) -> Option<AnyNodeId> {
    let through = match id.typed() {
        AnyId::EmptyStmt(_) => None,
        AnyId::ExprStmt(stmt) if diverges(tree.get(stmt).expr) => {
            Some(tree.get(stmt).expr.node_id())
        }
        AnyId::ParenExpr(paren) => tree.get(paren).inner.ok().map(|inner| inner.node_id()),
        AnyId::MatchParen(paren) if tree.get(paren).ascription.is_none() => {
            tree.get(paren).expr.ok().map(|inner| inner.node_id())
        }
        AnyId::ParenForInHeader(paren) => Some(AnyNodeId::of(tree.get(paren).header)),
        _ => return Some(id),
    };
    skipped.push(id);
    through.and_then(|inner| seen_through(tree, inner, skipped))
}

#[cfg(test)]
mod tests {
    use super::{Violation, check, excerpt, multiset_difference};

    fn texts(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn multiset_difference_counts_repeats() {
        let from = texts(&["// a", "// a", "// b", "// c"]);
        let minus = texts(&["// a", "// c", "// d"]);
        assert_eq!(multiset_difference(&from, &minus), texts(&["// a", "// b"]));
        assert_eq!(multiset_difference(&minus, &from), texts(&["// d"]));
        assert!(multiset_difference(&from, &from).is_empty());
    }

    #[test]
    fn excerpt_is_bounded_and_marks_what_it_leaves_out() {
        let text = "x".repeat(500);
        let shown = excerpt(&text, 300);
        assert!(shown.starts_with('…') && shown.ends_with('…'));
        assert!(shown.len() < 260);
        assert_eq!(excerpt("short", 2), "\"short\"");
    }

    #[test]
    fn a_lost_or_added_comment_is_a_comments_violation() {
        let input = baml_parser::parse("let x = 1; // c\n");
        assert!(input.errors.is_empty());
        let lost = check(&input.tree, "let x = 1;\n");
        assert_eq!(
            lost,
            Err(Violation::Comments {
                missing: texts(&["// c"]),
                added: Vec::new(),
            })
        );
        let moved = check(&input.tree, "// c\nlet x = 1;\n");
        assert_eq!(moved, Ok(()));
        let duplicated = check(&input.tree, "let x = 1; // c\n// c\n");
        assert_eq!(
            duplicated,
            Err(Violation::Comments {
                missing: Vec::new(),
                added: texts(&["// c"]),
            })
        );
    }

    #[test]
    fn a_comment_that_left_its_block_is_a_shape_violation() {
        let input = baml_parser::parse("function f() -> int {\n    // c\n    1\n}\n");
        assert!(input.errors.is_empty());
        let left = check(&input.tree, "// c\nfunction f() -> int {\n    1\n}\n");
        assert!(matches!(left, Err(Violation::Shape { .. })), "{left:?}");
    }
}
