//! What a tree costs in memory, on the parser stress file: the budget the
//! typed tree was chosen for (a few bytes per token and tens per node, well
//! under the rowan tree's), checked so a representation change that blows it
//! shows up here rather than in an editor.

use baml_parser::parse;
use baml_syntax::index::TreeIndex;

#[test]
fn stress_file_tree_stays_within_budget() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../baml_tests/projects/parser_stress/large_file.baml"
    );
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
    let parsed = parse(&text);
    assert!(parsed.errors.is_empty(), "the stress file parses cleanly");
    let tree = &parsed.tree;
    let tokens = tree.tokens().len();
    let nodes = tree.arenas().all_ids().len();
    let tree_bytes = tree.heap_bytes();
    let index = TreeIndex::build(tree);
    let index_bytes = index.heap_bytes();
    // The tree: the text, 5 bytes per token, and the nodes — about 25 bytes
    // of fields and 8 of span each, with the arenas' growth slack on top.
    let tree_budget = text.len() + 8 * tokens + 48 * nodes;
    assert!(
        tree_bytes <= tree_budget,
        "tree is {tree_bytes} B, budget {tree_budget} B ({} B of text, {tokens} tokens, {nodes} nodes)",
        text.len()
    );
    // The index: a parent per node and an owner per token, 8 bytes each.
    let index_budget = 8 * (nodes + tokens);
    assert!(
        index_bytes <= index_budget,
        "index is {index_bytes} B, budget {index_budget} B"
    );
}
