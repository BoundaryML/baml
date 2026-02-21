//! Qualified Names for unambiguous item identification.
//!
//! `QualifiedName` provides a way to uniquely identify items in the BAML project,
//! distinguishing between builtins, standard library items, user-defined items,
//! and (future) other modules and external packages.
//!
//! This is the canonical name type used across all compiler phases from TIR
//! through to runtime.

use crate::Name;

/// Prefix used for standard library items in qualified names.
///
/// All `Namespace::BamlStd` and standard `Namespace::Builtin` items
/// are displayed with this prefix (e.g., `baml.llm.call_llm_function`).
pub const BAML_STD_PREFIX: &str = "baml.";

/// Non-baml-prefixed builtin module names.
///
/// These are builtins that don't use the `baml.*` prefix convention.
/// For example, `env.get` instead of `baml.env.get`.
const NON_BAML_BUILTIN_PREFIXES: &[&str] = &["env"];

/// Check if a path starts with a non-baml builtin prefix.
fn is_non_baml_builtin_path(path: &[Name]) -> bool {
    path.first()
        .is_some_and(|first| NON_BAML_BUILTIN_PREFIXES.contains(&first.as_str()))
}

/// A qualified name that unambiguously identifies an item.
///
/// Combines a namespace (where the item lives) with a name (what the item
/// is called). This allows distinguishing between items with the same name
/// in different contexts.
///
/// # Why `QualifiedName` and not `FullyQualifiedName`?
///
/// BAML doesn't have partially qualified names - every name is either:
/// - A simple identifier resolved in the current scope, or
/// - A complete path that unambiguously identifies an item
///
/// Since there's no partial qualification, the "fully" prefix is redundant.
/// We use `QualifiedName` for brevity.
///
/// # Examples
///
/// ```ignore
/// // User-defined class "User"
/// QualifiedName { namespace: Namespace::Local, name: "User" }
///
/// // Builtin "env" (has property "get")
/// QualifiedName { namespace: Namespace::Builtin { path: [] }, name: "env" }
///
/// // Builtin method "Array.length"
/// QualifiedName { namespace: Namespace::Builtin { path: ["Array"] }, name: "length" }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::Update)]
pub struct QualifiedName {
    /// The namespace this item belongs to.
    pub namespace: Namespace,
    /// The item's name within its namespace.
    pub name: Name,
}

/// The namespace an item belongs to.
///
/// Namespaces organize items by their origin and resolution rules.
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::Update)]
pub enum Namespace {
    /// Compiler builtins that are "magic" - the compiler knows about them specially.
    ///
    /// Examples:
    /// - `env` -> `Builtin { path: [] }` with name "env"
    /// - `baml.Array.length` -> `Builtin { path: ["Array"] }` with name "length"
    Builtin {
        /// Path segments leading to the item.
        /// e.g., `["Array"]` for `baml.Array.length`
        /// e.g., `[]` for top-level builtins like `env`
        path: Vec<Name>,
    },

    /// Standard library items that require `baml.` prefix.
    ///
    /// Example: `baml.http.get` (future feature)
    BamlStd {
        /// Path segments after `baml.`
        /// e.g., `["http"]` for `baml.http.get`
        path: Vec<Name>,
    },

    /// User-defined items in the current project.
    ///
    /// This is the most common namespace. Items defined in `.baml` files
    /// without any module system or imports are all in this namespace.
    Local,

    /// Items from explicit user modules (future feature).
    ///
    /// When we add module support, items like `users.User` would have:
    /// `UserModule { module_path: ["users"] }`
    UserModule {
        /// The module path.
        /// e.g., `["users"]` for `users.User`
        module_path: Vec<Name>,
    },

    /// Items from external packages (future feature).
    ///
    /// When we add package support, items from dependencies would be
    /// in this namespace.
    Package {
        /// The package name.
        package_name: Name,
        /// The module path within the package.
        module_path: Vec<Name>,
    },
}

impl QualifiedName {
    /// Create a [`QualifiedName`] for a local (project-level) item.
    ///
    /// This is the most common constructor - use it for user-defined
    /// classes, enums, functions, etc.
    pub fn local(name: Name) -> Self {
        Self {
            namespace: Namespace::Local,
            name,
        }
    }

    /// Create a [`QualifiedName`] for a method on a local class.
    ///
    /// The method is identified by `ClassName.methodName` format.
    /// This is used for user-defined methods like `Baz.Greeting`.
    pub fn local_method(class_name: &Name, method_name: &Name) -> Self {
        Self {
            namespace: Namespace::Local,
            name: Self::local_method_from_str(class_name.as_str(), method_name.as_str()),
        }
    }

    /// Format a local method name from string parts.
    ///
    /// Returns the `Name` in `ClassName.methodName` format.
    /// This is the single source of truth for method name formatting.
    ///
    /// Use this when working with CST tokens (which provide `&str`).
    /// For `Name` inputs, prefer [`Self::local_method`] which returns a full [`QualifiedName`].
    pub fn local_method_from_str(class_name: &str, method_name: &str) -> Name {
        Name::new(format!("{class_name}.{method_name}"))
    }

    /// Get the display name (cached for repeated use).
    ///
    /// This is a convenience method that returns the result of `display()`
    /// as a `Name`. For most uses, calling `display()` directly is preferred.
    pub fn display_name(&self) -> Name {
        Name::new(self.display())
    }

    /// Get the module path as a vector of names.
    ///
    /// Returns the path segments for this qualified name:
    /// - Local: empty vector
    /// - Builtin: `["baml"]` + path segments (e.g., `["baml", "Array"]` for `baml.Array.length`)
    /// - `BamlStd`: `["baml"]` + path segments
    /// - `UserModule`: `module_path`
    /// - Package: `[package_name]` + `module_path`
    ///
    /// Note: For non-baml-prefixed builtins like `env.get`, returns `["env"]`.
    pub fn module_path(&self) -> Vec<Name> {
        match &self.namespace {
            Namespace::Local => vec![],
            Namespace::Builtin { path } => {
                // Check if this is a non-baml-prefixed builtin (e.g., "env.get")
                if is_non_baml_builtin_path(path) {
                    path.clone()
                } else {
                    let mut p = vec![Name::new("baml")];
                    p.extend(path.iter().cloned());
                    p
                }
            }
            Namespace::BamlStd { path } => {
                let mut p = vec![Name::new("baml")];
                p.extend(path.iter().cloned());
                p
            }
            Namespace::UserModule { module_path } => module_path.clone(),
            Namespace::Package {
                package_name,
                module_path,
            } => {
                let mut p = vec![package_name.clone()];
                p.extend(module_path.iter().cloned());
                p
            }
        }
    }

    /// Create a [`QualifiedName`] for a builtin item.
    ///
    /// # Arguments
    /// * `path` - The path segments leading to the item (e.g., `["Array"]`)
    /// * `name` - The item name (e.g., `"length"`)
    pub fn builtin(path: Vec<Name>, name: Name) -> Self {
        Self {
            namespace: Namespace::Builtin { path },
            name,
        }
    }

    /// Create a [`QualifiedName`] for a standard library item.
    pub fn baml_std(path: Vec<Name>, name: Name) -> Self {
        Self {
            namespace: Namespace::BamlStd { path },
            name,
        }
    }

    /// Create a [`QualifiedName`] for a user module item.
    pub fn user_module(module_path: Vec<Name>, name: Name) -> Self {
        Self {
            namespace: Namespace::UserModule { module_path },
            name,
        }
    }

    /// Create a [`QualifiedName`] for an external package item.
    pub fn package(package_name: Name, module_path: Vec<Name>, name: Name) -> Self {
        Self {
            namespace: Namespace::Package {
                package_name,
                module_path,
            },
            name,
        }
    }

    /// Check if this [`QualifiedName`] refers to a local item.
    pub fn is_local(&self) -> bool {
        matches!(self.namespace, Namespace::Local)
    }

    /// Check if this [`QualifiedName`] refers to a builtin.
    pub fn is_builtin(&self) -> bool {
        matches!(self.namespace, Namespace::Builtin { .. })
    }

    /// Check if this [`QualifiedName`] refers to a standard library item.
    pub fn is_baml_std(&self) -> bool {
        matches!(self.namespace, Namespace::BamlStd { .. })
    }

    /// Create a [`QualifiedName`] from module path segments and an item name.
    ///
    /// Used when resolving module item paths like `baml.http.Response`.
    pub fn from_module_path(module_path: &[Name], item_name: Name) -> Self {
        if module_path.is_empty() {
            Self::local(item_name)
        } else if module_path[0].as_str() == "baml" {
            // Path starts with "baml" - it's a builtin or baml_std item
            Self::builtin(module_path[1..].to_vec(), item_name)
        } else if is_non_baml_builtin_path(module_path) {
            // Non-baml builtin prefixes like "env"
            Self::builtin(module_path.to_vec(), item_name)
        } else {
            // User module path
            Self::user_module(module_path.to_vec(), item_name)
        }
    }

    /// Create a [`QualifiedName`] from path segments (e.g., `["baml", "Array", "length"]`).
    ///
    /// The last segment becomes the name, earlier segments become the path.
    pub fn from_path_segments(segments: &[Name]) -> Self {
        assert!(
            !segments.is_empty(),
            "cannot create QualifiedName from empty path"
        );
        if segments.len() == 1 {
            return Self::local(segments[0].clone());
        }

        let name = segments.last().unwrap().clone();
        let path = &segments[..segments.len() - 1];

        if path[0].as_str() == "baml" {
            // baml.* paths are builtins with the "baml" prefix stripped
            Self::builtin(path[1..].to_vec(), name)
        } else if is_non_baml_builtin_path(path) {
            // Non-baml builtins like "env.get" keep their prefix in the path
            Self::builtin(path.to_vec(), name)
        } else {
            // User module path
            Self::user_module(path.to_vec(), name)
        }
    }

    /// Create a [`QualifiedName`] for a builtin method on a receiver type.
    ///
    /// E.g., `builtin_method("image", "from_url")` creates a path like `baml.image.from_url`.
    pub fn builtin_method(receiver_type: Name, method_name: Name) -> Self {
        Self::builtin(vec![receiver_type], method_name)
    }

    /// Create a [`QualifiedName`] for a builtin primitive type (int, float, string, bool, etc.).
    ///
    /// These are simple builtins at the root level with no path.
    pub fn builtin_primitive(name: Name) -> Self {
        Self {
            namespace: Namespace::Builtin { path: vec![] },
            name,
        }
    }

    /// Get a display string for this [`QualifiedName`].
    ///
    /// Returns a human-readable representation like:
    /// - `"User"` for local items
    /// - `"baml.Array.length"` for builtins
    /// - `"env.get"` for non-baml-prefixed builtins
    pub fn display(&self) -> String {
        match &self.namespace {
            Namespace::Local => self.name.to_string(),
            Namespace::Builtin { path } => {
                // Check if the path starts with a non-baml module (e.g., "env")
                // These are stored with the module name in the path, so we don't add "baml."
                if is_non_baml_builtin_path(path) {
                    // e.g., path: ["env"], name: "get" -> "env.get"
                    let mut parts: Vec<&str> = path.iter().map(smol_str::SmolStr::as_str).collect();
                    parts.push(self.name.as_str());
                    parts.join(".")
                } else {
                    // Standard baml.* builtin
                    let mut parts: Vec<&str> = vec!["baml"];
                    parts.extend(path.iter().map(smol_str::SmolStr::as_str));
                    parts.push(self.name.as_str());
                    parts.join(".")
                }
            }
            Namespace::BamlStd { path } => {
                let mut parts: Vec<&str> = vec!["baml"];
                parts.extend(path.iter().map(smol_str::SmolStr::as_str));
                parts.push(self.name.as_str());
                parts.join(".")
            }
            Namespace::UserModule { module_path } => {
                let mut parts: Vec<&str> =
                    module_path.iter().map(smol_str::SmolStr::as_str).collect();
                parts.push(self.name.as_str());
                parts.join(".")
            }
            Namespace::Package {
                package_name,
                module_path,
            } => {
                let mut parts: Vec<&str> = vec![package_name.as_str()];
                parts.extend(module_path.iter().map(smol_str::SmolStr::as_str));
                parts.push(self.name.as_str());
                parts.join(".")
            }
        }
    }

    /// Convert to a runtime string for VM function lookup.
    ///
    /// This is the canonical string representation used by the VM to look up
    /// native functions. It should match the paths generated by `baml_builtins`.
    ///
    /// For builtins, this produces strings like:
    /// - `"baml.Array.length"`
    /// - `"baml.String.toLowerCase"`
    /// - `"env.get"`
    ///
    /// For local items, this is just the name.
    pub fn to_runtime_string(&self) -> String {
        self.display()
    }

    /// Parse a builtin path string into a [`QualifiedName`].
    ///
    /// Builtin paths follow the format:
    /// - `"baml.Array.length"` -> Builtin with path `["Array"]`, name: `"length"`
    /// - `"baml.http.Response.text"` -> Builtin with path `["http", "Response"]`, name: `"text"`
    /// - `"env.get"` -> Builtin with path `["env"]`, name: `"get"` (special non-baml prefix)
    /// - `"baml.deep_copy"` -> Builtin with path `[]`, name: `"deep_copy"`
    ///
    /// Note: Non-baml-prefixed paths like `"env.get"` are stored with the first segment
    /// as part of the path, so `to_runtime_string()` will produce the correct lookup key.
    ///
    /// # Panics
    /// Panics if the path is empty.
    pub fn from_builtin_path(path: &str) -> Self {
        let segments: Vec<&str> = path.split('.').collect();
        assert!(!segments.is_empty(), "builtin path cannot be empty");

        // Handle "env.get" style paths (no "baml." prefix)
        // These are special builtins that don't follow the baml.* convention
        if segments[0] != "baml" {
            // e.g., "env.get" -> NonBamlBuiltin { prefix: "env" }, name: "get"
            // For simplicity, we store as Builtin { path: ["env"] }, name: "get"
            // but display() needs special handling for these
            if segments.len() == 1 {
                // Just "env" or similar - treat as a single-segment builtin
                return Self {
                    namespace: Namespace::Builtin { path: vec![] },
                    name: Name::new(segments[0]),
                };
            }
            // "env.get" -> path: ["env"], name: "get"
            // This will display as "baml.env.get" but we need to track this is special
            // Actually, let's use a different approach - store the full path except last
            let path_segments: Vec<Name> = segments[..segments.len() - 1]
                .iter()
                .map(|s| Name::new(*s))
                .collect();
            let name = Name::new(segments[segments.len() - 1]);

            // Use BamlStd for non-baml-prefixed builtins to distinguish them
            // This way display() won't add extra "baml." prefix
            return Self {
                namespace: Namespace::Builtin {
                    path: path_segments,
                },
                name,
            };
        }

        // Handle "baml.*" paths
        if segments.len() == 1 {
            // Just "baml" - shouldn't happen but handle it
            return Self::builtin(vec![], Name::new("baml"));
        }

        // "baml.deep_copy" -> Builtin { path: [] }, name: "deep_copy"
        // "baml.Array.length" -> Builtin { path: ["Array"] }, name: "length"
        let path: Vec<Name> = segments[1..segments.len() - 1]
            .iter()
            .map(|s| Name::new(*s))
            .collect();
        let name = Name::new(segments[segments.len() - 1]);
        Self::builtin(path, name)
    }
}

impl std::fmt::Display for QualifiedName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display())
    }
}

impl std::fmt::Display for Namespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Namespace::Local => write!(f, "local"),
            Namespace::Builtin { path } => {
                write!(f, "builtin")?;
                if !path.is_empty() {
                    write!(
                        f,
                        ".{}",
                        path.iter()
                            .map(smol_str::SmolStr::as_str)
                            .collect::<Vec<_>>()
                            .join(".")
                    )?;
                }
                Ok(())
            }
            Namespace::BamlStd { path } => {
                write!(f, "baml")?;
                if !path.is_empty() {
                    write!(
                        f,
                        ".{}",
                        path.iter()
                            .map(smol_str::SmolStr::as_str)
                            .collect::<Vec<_>>()
                            .join(".")
                    )?;
                }
                Ok(())
            }
            Namespace::UserModule { module_path } => {
                write!(
                    f,
                    "mod.{}",
                    module_path
                        .iter()
                        .map(smol_str::SmolStr::as_str)
                        .collect::<Vec<_>>()
                        .join(".")
                )
            }
            Namespace::Package {
                package_name,
                module_path,
            } => {
                write!(f, "pkg.{package_name}")?;
                if !module_path.is_empty() {
                    write!(
                        f,
                        ".{}",
                        module_path
                            .iter()
                            .map(smol_str::SmolStr::as_str)
                            .collect::<Vec<_>>()
                            .join(".")
                    )?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_qn() {
        let qn = QualifiedName::local(Name::new("User"));
        assert!(qn.is_local());
        assert!(!qn.is_builtin());
        assert_eq!(qn.display(), "User");
        assert_eq!(qn.to_runtime_string(), "User");
    }

    #[test]
    fn test_builtin_qn() {
        let qn = QualifiedName::builtin(vec![Name::new("Array")], Name::new("length"));
        assert!(!qn.is_local());
        assert!(qn.is_builtin());
        assert_eq!(qn.display(), "baml.Array.length");
        assert_eq!(qn.to_runtime_string(), "baml.Array.length");
    }

    #[test]
    fn test_builtin_primitive() {
        let qn = QualifiedName::builtin_primitive(Name::new("int"));
        assert!(qn.is_builtin());
        assert_eq!(qn.display(), "baml.int");
    }

    #[test]
    fn test_baml_std() {
        let qn = QualifiedName::baml_std(vec![Name::new("http")], Name::new("get"));
        assert!(qn.is_baml_std());
        assert_eq!(qn.display(), "baml.http.get");
    }

    #[test]
    fn test_baml_std_prefix_matches_display() {
        assert_eq!(BAML_STD_PREFIX, "baml.");

        let builtin = QualifiedName::builtin(vec![Name::new("Array")], Name::new("length"));
        assert!(builtin.display().starts_with(BAML_STD_PREFIX));

        let std_item = QualifiedName::baml_std(vec![Name::new("http")], Name::new("get"));
        assert!(std_item.display().starts_with(BAML_STD_PREFIX));

        let env = QualifiedName::from_builtin_path("env.get");
        assert!(!env.display().starts_with(BAML_STD_PREFIX));
    }

    #[test]
    fn test_user_module() {
        let qn = QualifiedName::user_module(vec![Name::new("users")], Name::new("User"));
        assert!(!qn.is_local());
        assert_eq!(qn.display(), "users.User");
    }

    #[test]
    fn test_package() {
        let qn = QualifiedName::package(
            Name::new("external_pkg"),
            vec![Name::new("auth")],
            Name::new("Token"),
        );
        assert_eq!(qn.display(), "external_pkg.auth.Token");
    }

    #[test]
    fn test_qn_equality() {
        let qn1 = QualifiedName::local(Name::new("User"));
        let qn2 = QualifiedName::local(Name::new("User"));
        let qn3 = QualifiedName::local(Name::new("Admin"));

        assert_eq!(qn1, qn2);
        assert_ne!(qn1, qn3);
    }

    #[test]
    fn test_builtin_deep_path() {
        let qn = QualifiedName::builtin(
            vec![Name::new("http"), Name::new("Response")],
            Name::new("text"),
        );
        assert_eq!(qn.display(), "baml.http.Response.text");
        assert_eq!(qn.to_runtime_string(), "baml.http.Response.text");
    }

    #[test]
    fn test_from_builtin_path_array_method() {
        let qn = QualifiedName::from_builtin_path("baml.Array.length");
        assert!(qn.is_builtin());
        assert_eq!(qn.name.as_str(), "length");
        assert_eq!(qn.display(), "baml.Array.length");
    }

    #[test]
    fn test_from_builtin_path_deep() {
        let qn = QualifiedName::from_builtin_path("baml.http.Response.text");
        assert!(qn.is_builtin());
        assert_eq!(qn.name.as_str(), "text");
        assert_eq!(qn.display(), "baml.http.Response.text");
    }

    #[test]
    fn test_from_builtin_path_free_function() {
        let qn = QualifiedName::from_builtin_path("baml.deep_copy");
        assert!(qn.is_builtin());
        assert_eq!(qn.name.as_str(), "deep_copy");
        assert_eq!(qn.display(), "baml.deep_copy");
    }

    #[test]
    fn test_from_builtin_path_env() {
        let qn = QualifiedName::from_builtin_path("env.get");
        assert!(qn.is_builtin());
        assert_eq!(qn.name.as_str(), "get");
        // env.get is a non-baml-prefixed builtin, so it displays without baml. prefix
        assert_eq!(qn.display(), "env.get");
    }

    #[test]
    fn test_from_module_path_env() {
        // Non-baml builtin prefixes should be recognized as builtins
        let qn = QualifiedName::from_module_path(&[Name::new("env")], Name::new("get"));
        assert!(qn.is_builtin());
        assert_eq!(qn.display(), "env.get");
    }

    #[test]
    fn test_roundtrip_builtin_path() {
        // Ensure from_builtin_path and to_runtime_string roundtrip correctly
        let paths = [
            "baml.Array.length",
            "baml.String.toLowerCase",
            "baml.Map.keys",
            "baml.deep_copy",
            "baml.http.Response.text",
            "env.get", // non-baml-prefixed builtin
        ];
        for path in paths {
            let qn = QualifiedName::from_builtin_path(path);
            assert_eq!(qn.to_runtime_string(), path, "roundtrip failed for {path}");
        }
    }
}
