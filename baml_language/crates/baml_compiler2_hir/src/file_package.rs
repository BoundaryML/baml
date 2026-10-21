//! Package/namespace resolution for a source file.
//!
//! A file's package is its [`baml_base::SourceRoot`] (the root IS the
//! package); its namespace chain is derived from `ns_*` path segments
//! relative to the root's path.

use baml_base::{Name, SourceFile, SourceRoot};

/// Package/namespace info for a file.
#[derive(Debug, Clone, PartialEq, Eq, salsa::SalsaValue)]
pub struct PackageInfo {
    /// The package the file belongs to. Lets consumers ask `root.kind(db)`
    /// instead of sniffing path prefixes.
    pub root: SourceRoot,
    /// Namespace path within the package.
    /// e.g., `["llm"]` for `<builtin>/baml/ns_llm/llm.baml` or `ns_llm/client.baml`.
    pub namespace_path: Vec<Name>,
}

/// `file`'s path relative to its root: the spelling its namespace path, its
/// test initializer's name, and its unit's `source_file` are derived from, so
/// none of them carries the checkout's location — the same package compiles
/// to the same bytes wherever it lives. A file is under its root (the database
/// picks a path's root by prefix); the one other shape is a path that is
/// already relative, which the test harness adds to a root spelled `.`.
pub fn root_relative_path(db: &dyn crate::Db, file: SourceFile) -> std::path::PathBuf {
    let path = file.path(db);
    let root_path = file.source_root(db).path(db);
    match path.strip_prefix(root_path.as_path()) {
        Ok(relative) => relative.to_path_buf(),
        Err(_) => {
            debug_assert!(
                path.is_relative(),
                "`{}` is not under its root `{}`",
                path.display(),
                root_path.display()
            );
            path
        }
    }
}

/// Extract a namespace name from a path component if it has the `ns_` prefix
/// and a valid BAML identifier suffix (starts with letter or `_`, rest is
/// alphanumeric or `_`). Returns `None` for non-`ns_*` components or invalid suffixes.
fn extract_ns_name(component: &str) -> Option<Name> {
    let ns_name = component.strip_prefix("ns_")?;
    let mut chars = ns_name.chars();
    let valid = chars
        .next()
        .map(|c| c.is_ascii_alphabetic() || c == '_')
        .unwrap_or(false)
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then(|| Name::new(ns_name))
}

/// Determine which package a file belongs to.
///
/// Salsa-tracked (keyed on `file`) so the path parsing — `strip_prefix`,
/// component iteration, and the `Vec<Name>` allocation — runs once per file
/// instead of on every call. This is called from ~100 sites across every
/// compiler phase (often in per-class/per-function loops), so memoizing it
/// removes a pervasive, repeated path-parsing cost.
///
/// Reads only the file's `source_root` field and the root's `path` field:
/// adding or removing an unrelated root never invalidates a file's package
/// identity.
#[salsa::tracked(returns(clone))]
pub fn file_package(db: &dyn crate::Db, file: SourceFile) -> PackageInfo {
    let root = file.source_root(db);
    let relative = root_relative_path(db, file);

    let namespace_path: Vec<Name> = relative
        .parent()
        .map(|p| {
            p.components()
                .filter_map(|c| match c {
                    std::path::Component::Normal(name) => extract_ns_name(name.to_str()?),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();

    PackageInfo {
        root,
        namespace_path,
    }
}
