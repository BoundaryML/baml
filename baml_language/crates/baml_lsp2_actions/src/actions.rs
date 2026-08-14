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

use baml_base::SourceFile;
use baml_compiler_parser::syntax_tree;
use baml_compiler_syntax::{SyntaxElement, SyntaxKind, SyntaxNode, ast::StringLiteral};
use baml_compiler2_ast::ast::FunctionOrigin;
use baml_compiler2_hir::{contributions::Definition, file_symbol_contributions};
use rowan::ast::AstNode;
use text_size::TextRange;

use crate::Db;

// ── FileActionKind ────────────────────────────────────────────────────────────

/// The kind of action represented by a `FileAction`.
///
/// Maps to the LSP command that the caller (request.rs) will attach to the
/// `CodeLens` or `CodeAction` response.
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
pub fn file_actions(db: &dyn Db, file: SourceFile) -> Vec<FileAction> {
    let contribs = file_symbol_contributions(db, file);
    let mut actions = Vec::new();

    // Iterate value-namespace contributions: functions, tests, template strings,
    // clients, generators, retry policies all live here.
    for (name, contrib) in &contribs.values {
        match contrib.definition {
            Definition::Function(loc) => {
                let func = baml_compiler2_ppir::item_data::function_data(db, loc);
                if !matches!(func.metadata.origin, FunctionOrigin::UserDefined)
                    || func.metadata.is_language_internal
                {
                    continue;
                }

                actions.push(FileAction {
                    name: name.to_string(),
                    name_span: contrib.name_span,
                    kind: FileActionKind::RunInPlayground,
                });
            }
            Definition::Test(_) => {
                actions.push(FileAction {
                    name: name.to_string(),
                    name_span: contrib.name_span,
                    kind: FileActionKind::RunTest,
                });
            }
            // Other value-namespace items (client, generator, template_string,
            // retry_policy) don't get code lenses.
            _ => {}
        }
    }

    // New expr-body `test "..."` and `testset "..."` blocks are desugared into a
    // synthesized `$init_test` function during CST->AST lowering, so they never
    // appear as contributions. Enumerate them directly from the syntax tree.
    let tree = syntax_tree(db, file);
    collect_expr_body_test_actions(&tree, &mut actions);

    actions
}

fn collect_expr_body_test_actions(node: &SyntaxNode, actions: &mut Vec<FileAction>) {
    let action = match node.kind() {
        SyntaxKind::TEST_EXPR_DEF => Some((SyntaxKind::KW_TEST, FileActionKind::RunTest)),
        SyntaxKind::TESTSET_DEF => Some((SyntaxKind::KW_TESTSET, FileActionKind::RunTestSet)),
        _ => None,
    };

    if let Some((keyword, kind)) = action
        && let Some(name_el) = test_name_element(node, keyword)
    {
        let name = match name_el.as_node() {
            Some(n) => StringLiteral::cast(n.clone())
                .map(|s| s.value())
                .unwrap_or_else(|| n.text().to_string()),
            None => name_el
                .as_token()
                .map(|t| t.text().to_string())
                .unwrap_or_default(),
        };
        actions.push(FileAction {
            name,
            name_span: name_el.text_range(),
            kind,
        });
    }

    for child in node.children() {
        collect_expr_body_test_actions(&child, actions);
    }
}

/// Find the name element of a `test`/`testset` CST node: the first non-trivia
/// element after the keyword, stopping at a `with` clause or the body block.
fn test_name_element(node: &SyntaxNode, keyword: SyntaxKind) -> Option<SyntaxElement> {
    let mut past_keyword = false;
    for child in node.children_with_tokens() {
        let k = child.kind();
        if matches!(
            k,
            SyntaxKind::WHITESPACE
                | SyntaxKind::NEWLINE
                | SyntaxKind::LINE_COMMENT
                | SyntaxKind::BLOCK_COMMENT
                | SyntaxKind::HEADER_COMMENT
        ) {
            continue;
        }
        if k == keyword {
            past_keyword = true;
            continue;
        }
        if k == SyntaxKind::KW_WITH || k == SyntaxKind::BLOCK_EXPR {
            break;
        }
        if past_keyword {
            return Some(child);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::ProjectTest;

    #[test]
    fn file_actions_include_only_user_defined_functions() {
        let mut builder = ProjectTest::builder();
        builder.source(
            "main.baml",
            r##"
function Summarize(input: string) -> string {
    client: GPT4
    prompt: `Summarize ${input}`
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
        assert_eq!(playground_actions[0].name, "Summarize");
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
