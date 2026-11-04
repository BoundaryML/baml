//! The tree invariants, checked over every `.baml` file the repo owns:
//!
//! - every arena node is reachable from the root, and no node is referenced
//!   twice (unconditionally);
//! - no significant token is owned by two nodes (unconditionally);
//! - in a parse with no errors: every significant token is owned, and there
//!   is no `Missing` and no `Bogus`.
//!
//! Tokens inside a `Bogus`, `RemovedItem` head/body or `JinjaArg` body count
//! as owned by that node (its `Walk` reports them as one range).

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

#[derive(Default)]
struct Walk {
    /// Owner count per significant token.
    owners: BTreeMap<TokenIdx, u32>,
    /// Reference count per node.
    refs: BTreeMap<AnyNodeId, u32>,
    missing: Vec<Missing>,
    bogus: Vec<AnyNodeId>,
    pending: Vec<AnyNodeId>,
}

impl Visitor for Walk {
    fn node(&mut self, _field: &'static str, id: AnyNodeId) {
        self.pending.push(id);
    }
    fn token(&mut self, _field: &'static str, token: TokenIdx) {
        *self.owners.entry(token).or_default() += 1;
    }
    fn range(&mut self, _field: &'static str, range: TokenRange) {
        for token in range.iter() {
            *self.owners.entry(token).or_default() += 1;
        }
    }
    fn missing(&mut self, _field: &'static str, missing: Missing) {
        self.missing.push(missing);
    }
}

fn walk(tree: &FileTree) -> Walk {
    let arenas: &Arenas = tree.arenas();
    let mut w = Walk::default();
    let root = tree.root_id();
    *w.refs.entry(root).or_default() += 1;
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if Arenas::kind(id) == NodeKind::Bogus {
            w.bogus.push(id);
        }
        let before = w.pending.len();
        arenas.walk_node(id, &mut w);
        let children: Vec<_> = w.pending.drain(before..).collect();
        for child in children {
            *w.refs.entry(child).or_default() += 1;
            stack.push(child);
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
        if !parsed.errors.is_empty() {
            continue;
        }
        // Error-free: complete.
        for idx in tree.tokens().iter() {
            if !tree.tokens().kind(idx).is_trivia() && !w.owners.contains_key(&idx) {
                let range = tree.tokens().range(idx);
                let _ = writeln!(
                    report,
                    "{label}: unowned {:?} {:?} at {range:?}",
                    tree.tokens().kind(idx),
                    tree.token_text(idx)
                );
            }
        }
        for missing in &w.missing {
            let _ = writeln!(
                report,
                "{label}: Missing at {:?} with no error",
                missing.at()
            );
        }
        for id in &w.bogus {
            let span = tree.arenas().span(*id);
            let _ = writeln!(
                report,
                "{label}: Bogus at tokens {:?}..{:?} with no error",
                span.start(),
                span.end()
            );
        }
    }
    assert!(
        files > 1000,
        "expected the whole corpus, walked {files} files"
    );
    assert!(report.is_empty(), "tree invariant violations:\n{report}");
}
