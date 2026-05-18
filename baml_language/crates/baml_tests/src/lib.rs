//! Test utilities and automatic tests for the BAML compiler.
//!
//! This crate provides:
//! - [`engine`]: Unified test infrastructure using `baml_test!` macro
//! - Automatic snapshot tests generated from the `projects/{tier}/` directories by `build.rs`

pub mod engine;

/// Compile BAML source and run the entry function, returning bytecode display + result.
///
/// # Variants
///
/// Simple (source only, entry defaults to `"main"`, opt defaults to `OptLevel::One`):
/// ```ignore
/// baml_test!("source")
/// ```
///
/// Struct-style with named fields (all fields except `baml` are optional):
/// ```ignore
/// baml_test! {
///     baml: "source",
///     entry: "func",
///     args: { "x" => val },
///     opt: OptLevel::Zero,
/// }
/// ```
#[macro_export]
macro_rules! baml_test {
    // Simple: source only
    ($source:expr) => {
        $crate::engine::run_test(
            $source,
            "main",
            $crate::engine::IndexMap::new(),
            $crate::engine::OptLevel::One,
        )
        .await
    };
    // baml only
    (baml: $source:expr $(,)?) => {
        $crate::engine::run_test(
            $source,
            "main",
            $crate::engine::IndexMap::new(),
            $crate::engine::OptLevel::One,
        )
        .await
    };
    // baml + entry
    (baml: $source:expr, entry: $entry:expr $(,)?) => {
        $crate::engine::run_test(
            $source,
            $entry,
            $crate::engine::IndexMap::new(),
            $crate::engine::OptLevel::One,
        )
        .await
    };
    // baml + args
    (baml: $source:expr, args: { $($k:literal => $v:expr),* $(,)? } $(,)?) => {{
        let mut __args = $crate::engine::IndexMap::new();
        $( __args.insert($k, $v); )*
        $crate::engine::run_test(
            $source,
            "main",
            __args,
            $crate::engine::OptLevel::One,
        )
        .await
    }};
    // baml + entry + args
    (baml: $source:expr, entry: $entry:expr, args: { $($k:literal => $v:expr),* $(,)? } $(,)?) => {{
        let mut __args = $crate::engine::IndexMap::new();
        $( __args.insert($k, $v); )*
        $crate::engine::run_test(
            $source,
            $entry,
            __args,
            $crate::engine::OptLevel::One,
        )
        .await
    }};
    // baml + opt
    (baml: $source:expr, opt: $opt:expr $(,)?) => {
        $crate::engine::run_test(
            $source,
            "main",
            $crate::engine::IndexMap::new(),
            $opt,
        )
        .await
    };
    // baml + entry + opt
    (baml: $source:expr, entry: $entry:expr, opt: $opt:expr $(,)?) => {
        $crate::engine::run_test(
            $source,
            $entry,
            $crate::engine::IndexMap::new(),
            $opt,
        )
        .await
    };
    // baml + args + opt
    (baml: $source:expr, args: { $($k:literal => $v:expr),* $(,)? }, opt: $opt:expr $(,)?) => {{
        let mut __args = $crate::engine::IndexMap::new();
        $( __args.insert($k, $v); )*
        $crate::engine::run_test(
            $source,
            "main",
            __args,
            $opt,
        )
        .await
    }};
    // baml + entry + args + opt
    (baml: $source:expr, entry: $entry:expr, args: { $($k:literal => $v:expr),* $(,)? }, opt: $opt:expr $(,)?) => {{
        let mut __args = $crate::engine::IndexMap::new();
        $( __args.insert($k, $v); )*
        $crate::engine::run_test(
            $source,
            $entry,
            __args,
            $opt,
        )
        .await
    }};
    // baml + show_auto_derive (include synthesized to_json / from_json in the
    // bytecode snapshot — off by default to keep snapshots stable).
    (baml: $source:expr, show_auto_derive: $sad:expr $(,)?) => {
        $crate::engine::run_test_with_options(
            $source,
            "main",
            $crate::engine::IndexMap::new(),
            $crate::engine::OptLevel::One,
            $sad,
        )
        .await
    };
    // baml + entry + show_auto_derive
    (baml: $source:expr, entry: $entry:expr, show_auto_derive: $sad:expr $(,)?) => {
        $crate::engine::run_test_with_options(
            $source,
            $entry,
            $crate::engine::IndexMap::new(),
            $crate::engine::OptLevel::One,
            $sad,
        )
        .await
    };
}

/// Like `baml_test!` but at `OptLevel::Two` (includes MIR constant folding).
///
/// Use this for testing optimization passes like catch switch dispatch and
/// constant folding that require `OptLevel::Two`.
#[macro_export]
macro_rules! baml_test_optimized {
    // Simple: source only
    ($source:expr) => {
        $crate::engine::run_test_mir_optimized(
            $source,
            "main",
            $crate::engine::IndexMap::new(),
        )
        .await
    };
    // baml only
    (baml: $source:expr $(,)?) => {
        $crate::engine::run_test_mir_optimized(
            $source,
            "main",
            $crate::engine::IndexMap::new(),
        )
        .await
    };
    // baml + entry
    (baml: $source:expr, entry: $entry:expr $(,)?) => {
        $crate::engine::run_test_mir_optimized(
            $source,
            $entry,
            $crate::engine::IndexMap::new(),
        )
        .await
    };
    // baml + args
    (baml: $source:expr, args: { $($k:literal => $v:expr),* $(,)? } $(,)?) => {{
        let mut __args = $crate::engine::IndexMap::new();
        $( __args.insert($k, $v); )*
        $crate::engine::run_test_mir_optimized(
            $source,
            "main",
            __args,
        )
        .await
    }};
    // baml + entry + args
    (baml: $source:expr, entry: $entry:expr, args: { $($k:literal => $v:expr),* $(,)? } $(,)?) => {{
        let mut __args = $crate::engine::IndexMap::new();
        $( __args.insert($k, $v); )*
        $crate::engine::run_test_mir_optimized(
            $source,
            $entry,
            __args,
        )
        .await
    }};
}

#[cfg(test)]
macro_rules! assert_compiler2_snapshot {
    ($snapshot_path:expr, $name:expr, $output:expr) => {
        insta::with_settings!({ snapshot_path => $snapshot_path, omit_expression => true }, {
            insta::assert_snapshot!($name, $output);
        });
    };
}

#[cfg(test)]
pub mod compiler2_hir;

#[cfg(test)]
pub mod compiler2_tir;

#[cfg(test)]
pub mod compiler2_mir;

#[cfg(test)]
pub mod compiler2_emit;

#[cfg(test)]
pub mod incremental;

#[cfg(test)]
pub mod string_literals;

#[cfg(test)]
pub mod utils;

// Include the tests generated by build.rs.
// Written to src/ (not OUT_DIR) so file!() returns a stable path for insta snapshots.
#[cfg(test)]
include!("generated_tests.rs");

// Helper function for formatting syntax trees
#[cfg(test)]
fn format_syntax_tree(node: &baml_db::baml_compiler_syntax::SyntaxNode) -> String {
    format_node_recursive(node, 0)
}

#[cfg(test)]
fn format_node_recursive(node: &baml_db::baml_compiler_syntax::SyntaxNode, depth: usize) -> String {
    use baml_db::baml_compiler_syntax::NodeOrToken;

    let mut result = String::new();
    let indent = "  ".repeat(depth);

    result.push_str(&format!("{}{:?}", indent, node.kind()));

    // For leaf nodes (no child nodes), include the text
    if node.first_child().is_none() {
        let text = node.text().to_string().trim().to_string();
        if !text.is_empty() {
            // If text already has quotes, show as-is; otherwise wrap in quotes
            if text.starts_with('"') || text.starts_with("#\"") {
                result.push_str(&format!(" {}", text));
            } else {
                result.push_str(&format!(" \"{}\"", text));
            }
        }
    }

    result.push('\n');

    // Iterate over both nodes and tokens
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(child_node) => {
                result.push_str(&format_node_recursive(&child_node, depth + 1));
            }
            NodeOrToken::Token(token) => {
                // Show tokens (but skip trivia for readability)
                if !token.kind().is_trivia() {
                    result.push_str(&format!(
                        "{}{:?} \"{}\"\n",
                        "  ".repeat(depth + 1),
                        token.kind(),
                        token.text()
                    ));
                }
            }
        }
    }

    result
}
