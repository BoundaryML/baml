//! `file_actions` — code lenses for a file (Run in Playground, Run Test).
//!
//! This is a regular function (not a Salsa query). It uses
//! `file_symbol_contributions` to find all functions and tests defined in the
//! file, then produces one `FileAction` per item with an appropriate action
//! kind.
//!
//! ## Design
//!
//! Code lenses are purely structural — they don't need type inference. We only
//! need to know *where* each function and test is in the file, which
//! `file_symbol_contributions` already gives us via `name_span`.
//!
//! ## Action kinds
//!
//! - **`RunInPlayground`**: shown on every `function` item. Clicking it opens
//!   the BAML Playground for that function.
//! - **`RunTest`**: shown on every `test` item. Clicking it runs the test in
//!   the Playground.
//! - **`RunTestSet`**: shown on every `testset` block. Clicking it runs every
//!   test in the set.

use baml_base::SourceFile;
use baml_compiler2_ast::ast::FunctionOrigin;
use baml_compiler2_hir::{contributions::Definition, file_symbol_contributions};
use baml_syntax::{
    nodes::{AnyExpr, FileTree, TestDef},
    token::TokenKind,
    tree::AnyNode as _,
};
use text_size::TextRange;

// ── FileActionKind ────────────────────────────────────────────────────────────

/// The kind of action represented by a `FileAction`.
///
/// Maps to the LSP command that the caller will attach to the `CodeLens` or
/// `CodeAction` response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileActionKind {
    /// Open the BAML Playground focused on this function.
    RunInPlayground,
    /// Run this test case in the BAML Playground.
    RunTest,
    /// Run all tests in this testset.
    RunTestSet,
}

// ── FileAction ────────────────────────────────────────────────────────────────

/// A single code-lens action attached to a named item in the file.
///
/// The `name_span` gives the byte range of the item's name token — this is
/// what the LSP uses to position the code lens above the declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAction {
    /// The item's name as it appears in source.
    pub name: String,
    /// Byte range of the name token (used to position the code lens).
    pub name_span: TextRange,
    /// What kind of action this is.
    pub kind: FileActionKind,
}

// ── file_actions ──────────────────────────────────────────────────────────────

/// Return all code-lens actions for a file.
///
/// Regular function (not cached). Internally calls `file_symbol_contributions`,
/// which is Salsa-cached per file revision.
///
/// Returns one action per function (Run in Playground) and one per test (Run
/// Test), in the order they appear in the contributions list.
pub fn file_actions(db: &dyn baml_compiler2_hir::Db, file: SourceFile) -> Vec<FileAction> {
    let contribs = file_symbol_contributions(db, file);
    let mut actions = Vec::new();

    // Iterate value-namespace contributions: functions, tests, template strings,
    // clients, and retry policies all live here.
    for (name, contrib) in &contribs.values {
        match contrib.definition {
            Definition::Function(loc) => {
                let func = baml_compiler2_hir::item_data::function_data(db, loc);
                match func.metadata.origin {
                    FunctionOrigin::UserDefined => {}
                    FunctionOrigin::Companion
                    | FunctionOrigin::Internal
                    | FunctionOrigin::AutoDerive
                    | FunctionOrigin::TestInitializer => continue,
                }
                if func.metadata.is_language_internal {
                    continue;
                }

                actions.push(FileAction {
                    name: name.to_string(),
                    name_span: contrib.name_span,
                    kind: FileActionKind::RunInPlayground,
                });
            }
            // Other value-namespace items (top-level lets, client bindings
            // included) don't get code lenses; type definitions never appear
            // in the value namespace.
            Definition::Class(_)
            | Definition::Enum(_)
            | Definition::Interface(_)
            | Definition::TypeAlias(_)
            | Definition::Let(_) => {}
        }
    }

    // Expr-body `test "..."` / `testset "..."` blocks are desugared during
    // CST → AST lowering into ordinary `register_test*` calls inside a
    // synthesized `$init_test` function, so no contribution and no item-tree
    // item survives for them. The firewall consequently has no structural
    // record of a test block's *name and name span*: the name may even be a
    // computed expression (`"prefix:" + topic`), and the string-literal
    // spelling lives only in the syntax tree. Until the compiler records a
    // per-file test-registration index, enumerate the blocks directly from
    // the tree.
    let tree = baml_parser::typed_tree(db, file);
    collect_test_actions(&tree, &mut actions);

    actions
}

/// Every `test`/`testset` block of the tree, wherever it sits, in source
/// order of its name.
fn collect_test_actions(tree: &FileTree, actions: &mut Vec<FileAction>) {
    let tokens = tree.tokens();
    let mut found: Vec<FileAction> = tree
        .arenas()
        .ids::<TestDef>()
        .filter_map(|id| {
            let test = tree.get(id);
            let kind = if tokens.kind(test.kw.idx) == TokenKind::Test {
                FileActionKind::RunTest
            } else {
                FileActionKind::RunTestSet
            };
            let name = test.name.ok()?;
            let name_id = name
                .any_node_id()
                .unwrap_or_else(|| unreachable!("every alternative of `AnyExpr` is a node"));
            let name_span = tokens.text_range(tree.arenas().span(name_id));
            let text = match name {
                AnyExpr::String(lit) => string_value(tree.token_text(tree.get(lit).token.idx)),
                _ => tree.text()[name_span].to_string(),
            };
            Some(FileAction {
                name: text,
                name_span,
                kind,
            })
        })
        .collect();
    found.sort_by_key(|action| action.name_span.start());
    actions.extend(found);
}

/// The value of a string literal token: its body with escapes decoded, or
/// the raw body of an `r"…"`/`b"…"` literal.
fn string_value(text: &str) -> String {
    if let Some(body) = text.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        baml_base::escape::unescape_string_literal(body)
    } else if let Some(body) = text
        .strip_prefix("r\"")
        .or_else(|| text.strip_prefix("b\""))
        .and_then(|s| s.strip_suffix('"'))
    {
        body.to_string()
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ProjectTest;

    #[test]
    fn file_actions_include_only_user_defined_functions() {
        let mut builder = ProjectTest::builder();
        builder.source(
            "main.baml",
            r##"
function summarize(input: string) -> string {
    client: GPT4
    prompt: `summarize ${input}`
}
"##,
        );
        let project = builder.build();

        let actions = file_actions(&project.db, project.files[0]);
        let playground_actions: Vec<_> = actions
            .iter()
            .filter(|action| action.kind == FileActionKind::RunInPlayground)
            .collect();

        assert_eq!(playground_actions.len(), 1);
        assert_eq!(playground_actions[0].name, "summarize");
    }

    #[test]
    fn file_actions_include_tests_nested_in_testsets() {
        let mut builder = ProjectTest::builder();
        builder.source(
            "main.baml",
            r##"
testset "math" {
    test "adds" {
        assert.equal(1 + 1, 2)
    }

    testset "nested" {
        test "subtracts" {
            assert.equal(2 - 1, 1)
        }
    }
}
"##,
        );
        let project = builder.build();

        let actions = file_actions(&project.db, project.files[0]);
        let tests: Vec<_> = actions
            .iter()
            .filter(|action| action.kind == FileActionKind::RunTest)
            .map(|action| action.name.as_str())
            .collect();
        let testsets: Vec<_> = actions
            .iter()
            .filter(|action| action.kind == FileActionKind::RunTestSet)
            .map(|action| action.name.as_str())
            .collect();

        assert_eq!(tests, vec!["adds", "subtracts"]);
        assert_eq!(testsets, vec!["math", "nested"]);
    }
}
