//! The tree invariants, checked over every `.baml` file the repo owns:
//!
//! - every arena node is reachable from the root, and no node is referenced
//!   twice (unconditionally);
//! - no significant token is owned by two nodes (unconditionally);
//! - a node's children lie in source order, each after the one before it
//!   (unconditionally);
//! - in a parse with no errors: every significant token is owned, there is
//!   no `Missing` and no `Bogus`, a node's stored span is exactly the span of
//!   its children, and every item of a separated list but the last has its
//!   separator.
//!
//! Tokens inside a `Bogus`, `RemovedItem` head/body or `JinjaArg` body count
//! as owned by that node (its `Walk` reports them as one range). In a parse
//! with errors, recovery may skip tokens no node owns, and a node's span then
//! covers them: the stored span is checked against the children only when
//! nothing was skipped.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    path::{Path, PathBuf},
};

use baml_parser::parse;
use baml_syntax::{
    nodes::{Arenas, FileTree, NodeKind},
    token::TokenIdx,
    tree::{AnyNodeId, Missing, TokenRange, Visitor},
};

/// Every tree of `.baml` sources this repo owns, relative to this crate.
const ROOTS: [&str; 5] = [
    "../baml_tests/baml_src",
    "../baml_tests/projects",
    "../baml_builtins2/baml_std",
    "../../tools/type_quiz",
    "../baml_fmt/tests/cases",
];

fn source_files() -> Vec<PathBuf> {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for root in ROOTS {
        let root = crate_dir.join(root);
        let walk = walkdir::WalkDir::new(&root)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|entry| !entry.file_name().to_string_lossy().starts_with('.'));
        for entry in walk {
            let entry = entry.unwrap_or_else(|e| panic!("walking {}: {e}", root.display()));
            if entry.path().extension().and_then(|e| e.to_str()) == Some("baml") {
                files.push(entry.path().to_path_buf());
            }
        }
    }
    files
}

/// One part of a node, as its `Walk` reports it.
#[derive(Debug, Clone, Copy)]
enum Part {
    Node(&'static str, AnyNodeId),
    Token(&'static str, TokenIdx),
    Range(&'static str, TokenRange),
    Separator(&'static str, TokenIdx),
    /// An item of a separated list with no separator after it.
    Unseparated(&'static str),
    Missing(&'static str),
}

impl Part {
    /// The tokens the part covers, if any.
    fn extent(self, arenas: &Arenas) -> Option<TokenRange> {
        let range = match self {
            Part::Node(_, id) => arenas.span(id),
            Part::Token(_, t) | Part::Separator(_, t) => TokenRange::new(t, TokenIdx(t.0 + 1)),
            Part::Range(_, r) => r,
            Part::Unseparated(_) | Part::Missing(_) => return None,
        };
        (!range.is_empty()).then_some(range)
    }

    fn field(self) -> &'static str {
        match self {
            Part::Node(f, _)
            | Part::Token(f, _)
            | Part::Range(f, _)
            | Part::Separator(f, _)
            | Part::Unseparated(f)
            | Part::Missing(f) => f,
        }
    }
}

#[derive(Default)]
struct Parts(Vec<Part>);

impl Visitor for Parts {
    fn node(&mut self, field: &'static str, id: AnyNodeId) {
        self.0.push(Part::Node(field, id));
    }
    fn token(&mut self, field: &'static str, token: TokenIdx) {
        self.0.push(Part::Token(field, token));
    }
    fn separator(&mut self, field: &'static str, token: TokenIdx) {
        self.0.push(Part::Separator(field, token));
    }
    fn unseparated(&mut self, field: &'static str) {
        self.0.push(Part::Unseparated(field));
    }
    fn range(&mut self, field: &'static str, range: TokenRange) {
        self.0.push(Part::Range(field, range));
    }
    fn missing(&mut self, field: &'static str, missing: Missing) {
        let _ = missing;
        self.0.push(Part::Missing(field));
    }
}

/// What one walk of a tree found.
#[derive(Default)]
struct Walk {
    /// Owner count per significant token.
    owners: BTreeMap<TokenIdx, u32>,
    /// Reference count per node.
    refs: BTreeMap<AnyNodeId, u32>,
    missing: Vec<AnyNodeId>,
    bogus: Vec<AnyNodeId>,
    /// Nodes whose children are out of source order.
    disordered: Vec<AnyNodeId>,
    /// Nodes whose stored span differs from their children's.
    misspanned: Vec<(AnyNodeId, TokenRange, Option<TokenRange>)>,
    /// Nodes with a separated-list item after one with no separator.
    unseparated: Vec<(AnyNodeId, &'static str)>,
}

fn walk(tree: &FileTree) -> Walk {
    let arenas: &Arenas = tree.arenas();
    let mut w = Walk::default();
    let root = tree.root_id();
    *w.refs.entry(root).or_default() += 1;
    let mut stack = vec![root];
    let mut parts = Parts::default();
    while let Some(id) = stack.pop() {
        if Arenas::kind(id) == NodeKind::Bogus {
            w.bogus.push(id);
        }
        parts.0.clear();
        arenas.walk_node(id, &mut parts);

        let mut derived: Option<TokenRange> = None;
        let mut last_unseparated: Option<&'static str> = None;
        for &part in &parts.0 {
            match part {
                Part::Node(_, child) => {
                    *w.refs.entry(child).or_default() += 1;
                    stack.push(child);
                }
                Part::Token(_, t) | Part::Separator(_, t) => {
                    *w.owners.entry(t).or_default() += 1;
                }
                Part::Range(_, r) => {
                    for t in r.iter() {
                        *w.owners.entry(t).or_default() += 1;
                    }
                }
                Part::Missing(_) => w.missing.push(id),
                Part::Unseparated(_) => {}
            }
            if let Some(extent) = part.extent(arenas) {
                derived = Some(match derived {
                    None => extent,
                    Some(so_far) => {
                        if extent.start() < so_far.end() {
                            w.disordered.push(id);
                        }
                        TokenRange::new(so_far.start(), extent.end())
                    }
                });
            }
            match (part, last_unseparated) {
                (Part::Unseparated(field), _) => last_unseparated = Some(field),
                (_, Some(field)) if part.field() == field => w.unseparated.push((id, field)),
                _ => {}
            }
        }
        let stored = arenas.span(id);
        if derived.is_none_or(|d| d != stored) && !(derived.is_none() && stored.is_empty()) {
            w.misspanned.push((id, stored, derived));
        }
    }
    w
}

#[test]
fn tree_invariants_hold_on_every_corpus_file() {
    let mut report = String::new();
    let mut files = 0usize;
    for path in source_files() {
        files += 1;
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        let parsed = parse(&text);
        let tree = &parsed.tree;
        let w = walk(tree);
        let label = path.display();

        // Unconditional: reachability and single reference.
        let reachable: BTreeSet<AnyNodeId> = w.refs.keys().copied().collect();
        for id in tree.arenas().all_ids() {
            if !reachable.contains(&id) {
                let _ = writeln!(report, "{label}: unreferenced {:?}", Arenas::kind(id));
            }
        }
        for (id, n) in &w.refs {
            if *n > 1 {
                let _ = writeln!(
                    report,
                    "{label}: {:?} referenced {n} times",
                    Arenas::kind(*id)
                );
            }
        }
        // Unconditional: no token owned twice.
        for (idx, n) in &w.owners {
            if *n > 1 {
                let range = tree.tokens().range(*idx);
                let _ = writeln!(
                    report,
                    "{label}: token {:?} at {range:?} owned {n} times",
                    tree.tokens().kind(*idx)
                );
            }
        }
        // Unconditional: children in source order.
        for id in &w.disordered {
            let _ = writeln!(
                report,
                "{label}: {:?} at {:?} has children out of order",
                Arenas::kind(*id),
                tree.arenas().span(*id)
            );
        }
        if !parsed.errors.is_empty() {
            continue;
        }
        // Error-free: complete.
        for idx in tree.tokens().iter() {
            if !tree.tokens().kind(idx).is_trivia() && !w.owners.contains_key(&idx) {
                let range = tree.tokens().range(idx);
                let _ = writeln!(
                    report,
                    "{label}: token {:?} at {range:?} is owned by no node",
                    tree.tokens().kind(idx)
                );
            }
        }
        for id in &w.missing {
            let _ = writeln!(
                report,
                "{label}: {:?} has a Missing child in an error-free parse",
                Arenas::kind(*id)
            );
        }
        for id in &w.bogus {
            let _ = writeln!(
                report,
                "{label}: Bogus at {:?} in an error-free parse",
                tree.arenas().span(*id)
            );
        }
        // Error-free: the stored span is the children's span.
        for (id, stored, derived) in &w.misspanned {
            let _ = writeln!(
                report,
                "{label}: {:?} stores span {stored:?} but its children span {derived:?}",
                Arenas::kind(*id)
            );
        }
        // Error-free: every item but the last has its separator.
        for (id, field) in &w.unseparated {
            let _ = writeln!(
                report,
                "{label}: {:?}.{field} has an item after one with no separator",
                Arenas::kind(*id)
            );
        }
    }
    assert!(
        files > 500,
        "walked only {files} files: is the corpus where ROOTS says?"
    );
    assert!(report.is_empty(), "tree invariants violated:\n{report}");
}
