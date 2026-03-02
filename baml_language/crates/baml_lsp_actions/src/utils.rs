//! Shared utilities for LSP actions.

use baml_db::{
    SourceFile,
    baml_compiler_hir::{self, FunctionLoc, ItemId},
    baml_compiler_parser, baml_compiler_syntax,
};
use rowan::ast::AstNode;
use text_size::TextSize;

/// Find the function whose syntax range contains the given offset.
///
/// Checks both top-level functions and class methods.
pub fn find_function_at_position(
    db: &dyn baml_compiler_hir::Db,
    file: SourceFile,
    position: TextSize,
) -> Option<FunctionLoc<'_>> {
    let items = baml_compiler_hir::file_items(db, file);
    let tree = baml_compiler_parser::syntax_tree(db, file);
    let ast_file = baml_compiler_syntax::ast::SourceFile::cast(tree)?;

    for item_id in items.items(db) {
        if let ItemId::Function(func_loc) = item_id {
            let item_tree = baml_compiler_hir::file_item_tree(db, file);
            let func = &item_tree[func_loc.id(db)];
            let func_name = &func.name;

            for item in ast_file.items() {
                match item {
                    baml_compiler_syntax::ast::Item::Function(func_node) => {
                        if let Some(name) = func_node.name() {
                            if name.text() == func_name {
                                if func_node.syntax().text_range().contains(position) {
                                    return Some(*func_loc);
                                }
                            }
                        }
                    }
                    baml_compiler_syntax::ast::Item::Class(class_node) => {
                        let class_name = class_node
                            .name()
                            .map(|n| n.text().to_string())
                            .unwrap_or_else(|| "UnnamedClass".to_string());
                        for method in class_node.methods() {
                            if let Some(name) = method.name() {
                                let qualified =
                                    baml_compiler_hir::QualifiedName::local_method_from_str(
                                        &class_name,
                                        name.text(),
                                    );
                                if qualified.as_str() == func_name.as_str() {
                                    if method.syntax().text_range().contains(position) {
                                        return Some(*func_loc);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    None
}
