//! Fully Qualified Names for unambiguous item identification.
//!
//! FQNs provide a way to uniquely identify items in the BAML project,
//! distinguishing between builtins, standard library items, user-defined
//! items, and (future) other modules and external packages.

use baml_base::Name;

/// A fully-qualified name that unambiguously identifies an item.
///
/// FQNs combine a namespace (where the item lives) with a name (what the item
/// is called). This allows distinguishing between items with the same name
/// in different contexts.
///
/// # Examples
///
/// ```ignore
/// // User-defined class "User"
/// FQN { namespace: Namespace::Local, name: "User" }
///
/// // Builtin "env" (has property "get")
/// FQN { namespace: Namespace::Builtin { path: [] }, name: "env" }
///
/// // Builtin method "Array.length"
/// FQN { namespace: Namespace::Builtin { path: ["Array"] }, name: "length" }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::Update)]
pub struct FullyQualifiedName {
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

impl FullyQualifiedName {
    /// Create an FQN for a local (project-level) item.
    ///
    /// This is the most common constructor - use it for user-defined
    /// classes, enums, functions, etc.
    pub fn local(name: Name) -> Self {
        Self {
            namespace: Namespace::Local,
            name,
        }
    }

    /// Create an FQN for a builtin item.
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

    /// Create an FQN for a standard library item.
    pub fn baml_std(path: Vec<Name>, name: Name) -> Self {
        Self {
            namespace: Namespace::BamlStd { path },
            name,
        }
    }

    /// Check if this FQN refers to a local item.
    pub fn is_local(&self) -> bool {
        matches!(self.namespace, Namespace::Local)
    }

    /// Check if this FQN refers to a builtin.
    pub fn is_builtin(&self) -> bool {
        matches!(self.namespace, Namespace::Builtin { .. })
    }

    /// Create an FQN for a builtin primitive type (int, float, string, bool, etc.).
    ///
    /// These are simple builtins at the root level with no path.
    pub fn builtin_primitive(name: Name) -> Self {
        Self {
            namespace: Namespace::Builtin { path: vec![] },
            name,
        }
    }

    /// Get a display string for this FQN.
    ///
    /// Returns a human-readable representation like:
    /// - `"User"` for local items
    /// - `"baml.Array.length"` for builtins
    pub fn display(&self) -> String {
        match &self.namespace {
            Namespace::Local => self.name.to_string(),
            Namespace::Builtin { path } => {
                let mut parts: Vec<&str> = vec!["baml"];
                parts.extend(path.iter().map(smol_str::SmolStr::as_str));
                parts.push(self.name.as_str());
                parts.join(".")
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
}

impl std::fmt::Display for FullyQualifiedName {
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
    fn test_local_fqn() {
        let fqn = FullyQualifiedName::local(Name::new("User"));
        assert!(fqn.is_local());
        assert!(!fqn.is_builtin());
        assert_eq!(fqn.display(), "User");
    }

    #[test]
    fn test_builtin_fqn() {
        let fqn = FullyQualifiedName::builtin(vec![Name::new("Array")], Name::new("length"));
        assert!(!fqn.is_local());
        assert!(fqn.is_builtin());
        assert_eq!(fqn.display(), "baml.Array.length");
    }

    #[test]
    fn test_fqn_equality() {
        let fqn1 = FullyQualifiedName::local(Name::new("User"));
        let fqn2 = FullyQualifiedName::local(Name::new("User"));
        let fqn3 = FullyQualifiedName::local(Name::new("Admin"));

        assert_eq!(fqn1, fqn2);
        assert_ne!(fqn1, fqn3);
    }
}
