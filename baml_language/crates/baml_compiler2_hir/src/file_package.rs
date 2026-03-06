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
    /// Namespace path within the package. Empty for user files.
    /// e.g., `["llm"]` for `<builtin>/baml/llm.baml`.
    pub namespace_path: Vec<Name>,
}

/// Determine which package a file belongs to based on its path.
pub fn file_package(db: &dyn crate::Db, file: SourceFile) -> PackageInfo {
    let path = file.path(db);
    let path_str = path.to_string_lossy();

    if let Some(relative) = path_str.strip_prefix("<builtin>/") {
        // <builtin>/baml/llm.baml → package "baml", namespace ["llm"]
        // <builtin>/env.baml → package "env", namespace []
        let segments: Vec<&str> = relative.split('/').collect();
        if segments.len() >= 2 {
            let package = Name::new(segments[0]);
            let ns_path: Vec<Name> = segments[1..segments.len() - 1]
                .iter()
                .map(|s| Name::new(*s))
                .collect();
            PackageInfo {
                package,
                namespace_path: ns_path,
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
        // All user files → package "user", no namespace
        PackageInfo {
            package: Name::new("user"),
            namespace_path: vec![],
        }
    }
}
