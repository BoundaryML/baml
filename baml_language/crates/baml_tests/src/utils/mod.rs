//! Test utilities for parser testing, especially incremental parsing verification.

use std::collections::HashSet;

use baml_db::baml_syntax::SyntaxNode;

/// Metrics for measuring node reuse in incremental parsing
#[derive(Debug)]
pub struct ReuseMetrics {
    pub total_old_nodes: usize,
    pub total_new_nodes: usize,
    pub reused_nodes: usize,
    pub reuse_percentage: f64,
}

/// Verify the parse tree can reconstruct the original source exactly
pub fn assert_tree_is_lossless(tree: &SyntaxNode, original: &str) {
    let reconstructed = tree.to_string();
    assert_eq!(
        reconstructed, original,
        "Tree is not lossless: reconstruction doesn't match original"
    );
}

/// Test that no panics occur when traversing the tree
pub fn assert_no_panics(tree: &SyntaxNode) {
    fn traverse(node: &SyntaxNode) {
        // Access all node properties to ensure no panics
        let _ = node.kind();
        let _ = node.text();
        let _ = node.text_range();

        for child in node.children() {
            traverse(&child);
        }
    }

    traverse(tree);
}

/// Insert a character at the given position in a string
pub fn insert_char(source: &str, pos: usize, ch: char) -> String {
    let mut result = String::new();
    let chars: Vec<char> = source.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if i == pos {
            result.push(ch);
        }
        result.push(c);
    }

    if pos == chars.len() {
        result.push(ch);
    }

    result
}

/// Delete a character at the given position
pub fn delete_char(source: &str, pos: usize) -> String {
    let mut result = String::new();
    let chars: Vec<char> = source.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if i != pos {
            result.push(c);
        }
    }

    result
}

/// Replace a character at the given position
pub fn replace_char(source: &str, pos: usize, ch: char) -> String {
    let mut result = String::new();
    let chars: Vec<char> = source.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if i == pos {
            result.push(ch);
        } else {
            result.push(c);
        }
    }

    result
}

/// Test all single-character edits and verify incremental parsing
#[allow(unused_variables)]
pub fn test_all_single_char_edits<F>(source: &str, parse_fn: F, parse_incremental_fn: F)
where
    F: Fn(&str) -> SyntaxNode,
{
    let original_tree = parse_fn(source);

    // Test adding each ASCII char at each position
    for pos in 0..=source.len() {
        for ch in b' '..=b'~' {
            let modified = insert_char(source, pos, ch as char);
            let incremental_tree = parse_incremental_fn(&modified);
            let full_reparse = parse_fn(&modified);

            assert_trees_equivalent(&incremental_tree, &full_reparse);
        }
    }

    // Test deleting each character
    for pos in 0..source.len() {
        let modified = delete_char(source, pos);
        let incremental_tree = parse_incremental_fn(&modified);
        let full_reparse = parse_fn(&modified);

        assert_trees_equivalent(&incremental_tree, &full_reparse);
    }

    // Test replacing each character
    for pos in 0..source.len() {
        for ch in b' '..=b'~' {
            let modified = replace_char(source, pos, ch as char);
            let incremental_tree = parse_incremental_fn(&modified);
            let full_reparse = parse_fn(&modified);

            assert_trees_equivalent(&incremental_tree, &full_reparse);
        }
    }
}

/// Assert that two syntax trees are structurally equivalent
pub fn assert_trees_equivalent(tree1: &SyntaxNode, tree2: &SyntaxNode) {
    fn compare_nodes(node1: &SyntaxNode, node2: &SyntaxNode, path: &str) {
        assert_eq!(
            node1.kind(),
            node2.kind(),
            "Node kinds differ at path: {}",
            path
        );

        assert_eq!(
            node1.text(),
            node2.text(),
            "Node text differs at path: {}",
            path
        );

        let children1: Vec<_> = node1.children().collect();
        let children2: Vec<_> = node2.children().collect();

        assert_eq!(
            children1.len(),
            children2.len(),
            "Child count differs at path: {}",
            path
        );

        for (i, (child1, child2)) in children1.iter().zip(children2.iter()).enumerate() {
            let child_path = format!("{}/{:?}[{}]", path, child1.kind(), i);
            compare_nodes(child1, child2, &child_path);
        }
    }

    compare_nodes(tree1, tree2, "root");
}

/// Collect pointer addresses of all nodes in a tree (for reuse measurement)
fn collect_node_pointers(node: &SyntaxNode) -> HashSet<usize> {
    let mut pointers = HashSet::new();

    fn collect(node: &SyntaxNode, pointers: &mut HashSet<usize>) {
        // Get the raw pointer address of this node
        let ptr = node as *const _ as usize;
        pointers.insert(ptr);

        for child in node.children() {
            collect(&child, pointers);
        }
    }

    collect(node, &mut pointers);
    pointers
}

/// Measure node reuse percentage between old and new trees
pub fn measure_node_reuse(old_tree: &SyntaxNode, new_tree: &SyntaxNode) -> ReuseMetrics {
    let old_nodes = collect_node_pointers(old_tree);
    let new_nodes = collect_node_pointers(new_tree);

    let reused = old_nodes.intersection(&new_nodes).count();

    ReuseMetrics {
        total_old_nodes: old_nodes.len(),
        total_new_nodes: new_nodes.len(),
        reused_nodes: reused,
        reuse_percentage: (reused as f64 / old_nodes.len().max(1) as f64) * 100.0,
    }
}

/// Common edit patterns to test
pub struct EditPattern {
    pub name: &'static str,
    pub apply: fn(&str) -> String,
}

/// Test common edit patterns on a file
pub fn test_common_edit_patterns<F>(source: &str, parse_fn: F, parse_incremental_fn: F)
where
    F: Fn(&str) -> SyntaxNode,
{
    let patterns = vec![
        EditPattern {
            name: "add_newline_at_end",
            apply: |s| format!("{}\n", s),
        },
        EditPattern {
            name: "add_comment_at_start",
            apply: |s| format!("// New comment\n{}", s),
        },
        EditPattern {
            name: "add_field_to_first_class",
            apply: |s| {
                // Simple heuristic: find first '{' after 'class' and insert field
                if let Some(class_pos) = s.find("class ")
                    && let Some(brace_pos) = s[class_pos..].find('{')
                {
                    let insert_pos = class_pos + brace_pos + 1;
                    let mut result = String::from(&s[..insert_pos]);
                    result.push_str("\n  newField string");
                    result.push_str(&s[insert_pos..]);
                    return result;
                }
                s.to_string()
            },
        },
        EditPattern {
            name: "change_string_literal",
            apply: |s| {
                // Find first string literal and change it
                if let Some(quote_pos) = s.find('"')
                    && let Some(end_pos) = s[quote_pos + 1..].find('"')
                {
                    let mut result = String::from(&s[..quote_pos + 1]);
                    result.push_str("modified");
                    result.push_str(&s[quote_pos + 1 + end_pos..]);
                    return result;
                }
                s.to_string()
            },
        },
    ];

    for pattern in patterns {
        let modified = (pattern.apply)(source);
        let incremental_tree = parse_incremental_fn(&modified);
        let full_reparse = parse_fn(&modified);

        assert_trees_equivalent(&incremental_tree, &full_reparse);
    }
}

/// Measure incremental parsing performance for different edit sizes
pub fn measure_incremental_performance<F>(
    source: &str,
    parse_fn: F,
    parse_incremental_fn: F,
) -> Vec<(usize, ReuseMetrics)>
where
    F: Fn(&str) -> SyntaxNode,
{
    let mut results = Vec::new();
    let original_tree = parse_fn(source);

    // Single character edit
    let modified_1 = insert_char(source, source.len() / 2, 'x');
    let tree_1 = parse_incremental_fn(&modified_1);
    results.push((1, measure_node_reuse(&original_tree, &tree_1)));

    // Small edit (add a line)
    let modified_10 = format!("{}\n// New line", source);
    let tree_10 = parse_incremental_fn(&modified_10);
    results.push((10, measure_node_reuse(&original_tree, &tree_10)));

    // Medium edit (add a class)
    let modified_100 = format!("{}\n\nclass NewClass {{\n  field string\n}}", source);
    let tree_100 = parse_incremental_fn(&modified_100);
    results.push((100, measure_node_reuse(&original_tree, &tree_100)));

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_edit_functions() {
        let source = "hello world";

        assert_eq!(insert_char(source, 0, 'X'), "Xhello world");
        assert_eq!(insert_char(source, 5, 'X'), "helloX world");
        assert_eq!(insert_char(source, 11, 'X'), "hello worldX");

        assert_eq!(delete_char(source, 0), "ello world");
        assert_eq!(delete_char(source, 5), "helloworld");

        assert_eq!(replace_char(source, 0, 'X'), "Xello world");
        assert_eq!(replace_char(source, 6, 'X'), "hello Xorld");
    }
}
