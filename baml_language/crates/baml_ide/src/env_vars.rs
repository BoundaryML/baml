//! Collect environment variable references across a BAML project.
//!
//! Uses the `env_var_refs` stored in `FileSemanticIndex` — populated during
//! CST → AST lowering when `env.X` expressions are desugared.

/// Collect all unique env var names referenced across all project files.
///
/// Returns a sorted, deduplicated list of variable names (e.g.,
/// `["ANTHROPIC_API_KEY", "OPENAI_API_KEY"]`).
pub fn all_env_var_names(db: &dyn baml_compiler2_ppir::Db) -> Vec<String> {
    let files = baml_compiler2_hir::compiler2_all_files(db);
    let mut names = std::collections::BTreeSet::new();
    for file in files {
        db.unwind_if_revision_cancelled();
        for env_ref in baml_compiler2_hir::file_env_var_refs(db, file) {
            names.insert(env_ref.name.clone());
        }
    }
    names.into_iter().collect()
}
