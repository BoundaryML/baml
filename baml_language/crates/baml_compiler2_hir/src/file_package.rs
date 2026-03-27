//! Package/namespace resolution for a source file.
//!
//! Determines which package and namespace chain a file belongs to based on
//! its path. User files → `package: "user"`, built-in files → `package: "baml"`
//! or `"env"` based on the `<builtin>/` prefix.

use baml_base::{Name, SourceFile};

/// Package/namespace info for a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInfo {
    /// Package name: "user", "baml", or "env".
    pub package: Name,
    /// Namespace path within the package.
    /// e.g., `["llm"]` for `<builtin>/baml/ns_llm/llm.baml` or `ns_llm/client.baml`.
    pub namespace_path: Vec<Name>,
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

/// Determine which package a file belongs to based on its path.
pub fn file_package(db: &dyn crate::Db, file: SourceFile) -> PackageInfo {
    let path = file.path(db);
    let path_str = path.to_string_lossy();

    if let Some(relative) = path_str.strip_prefix("<builtin>/") {
        let segments: Vec<&str> = relative.split('/').collect();
        if segments.len() >= 2 {
            let package = Name::new(segments[0]);
            // Apply ns_* detection to intermediate segments (same as user files)
            let namespace_path: Vec<Name> = segments[1..segments.len() - 1]
                .iter()
                .filter_map(|s| extract_ns_name(s))
                .collect();
            PackageInfo {
                package,
                namespace_path,
            }
        } else {
            // e.g. <builtin>/env.baml → package "env"
            let stem = segments[0].trim_end_matches(".baml");
            PackageInfo {
                package: Name::new(stem),
                namespace_path: vec![],
            }
        }
    } else {
        // User files: derive namespace from ns_* folder segments.
        let root = db.project().root(db);
        let path = std::path::Path::new(path_str.as_ref());
        let relative = path
            .strip_prefix(root.as_path())
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|_| path.to_path_buf());

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
            package: Name::new("user"),
            namespace_path,
        }
    }
}
