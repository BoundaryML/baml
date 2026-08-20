//! Qualified type names and package identity.
//!
//! [`QualifiedTypeName`] identifies a class/enum/type-alias by its definition's
//! package and short name; [`Package`] distinguishes the user's own (implicit
//! root) package from named dependencies.

use std::fmt;

use baml_base::Name;
use borsh::{BorshDeserialize, BorshSerialize};

use crate::{BuiltinTypeName, PrimitiveType};

/// Which package a type is defined in. `Local` is the user's own (implicit
/// root) package — the "current" package for everything a user writes;
/// `Dep(name)` is a named dependency (e.g. `baml`). Encoding this as a type
/// rather than a magic `"user"` string means the local-vs-dependency
/// distinction is checked by the compiler, not by string comparison: the only
/// place the `"user"` string appears is [`Package::from_name`] (the boundary
/// where upstream `Name`-based package info is classified).
#[derive(Debug, Clone, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum Package {
    /// The user's own implicit root package (`RESERVED_USER_PACKAGE`).
    Local,
    /// A named dependency package.
    Dep(Name),
}

/// The interned `Name` of the reserved implicit `user` package, materialized
/// once so [`QualifiedTypeName::package`] can hand out a `&Name` for `Local`.
static USER_PACKAGE_NAME: Name = Name::new_inline(RESERVED_USER_PACKAGE);

impl Package {
    /// Classify an upstream package `Name`: the reserved `user` package becomes
    /// [`Package::Local`], everything else a [`Package::Dep`]. This is the one
    /// spot the `"user"` magic string is read.
    pub fn from_name(name: Name) -> Self {
        if name.as_str() == RESERVED_USER_PACKAGE {
            Package::Local
        } else {
            Package::Dep(name)
        }
    }

    /// The package's `Name` (`Local` resolves to the reserved `user` name).
    pub fn as_name(&self) -> &Name {
        match self {
            Package::Local => &USER_PACKAGE_NAME,
            Package::Dep(name) => name,
        }
    }
}

// Order/sort by the package *name* string, preserving the pre-enum `Ord`
// (where `pkg` was a `Name`) so `QualifiedTypeName`'s derived ordering — and
// any sorted output keyed on it — is unchanged.
impl Ord for Package {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_name().as_str().cmp(other.as_name().as_str())
    }
}

impl PartialOrd for Package {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// A qualified type name with separate package and local name.
///
/// Used in `Ty::Class`, `Ty::Enum`, and `Ty::TypeAlias` to unambiguously
/// identify a type by its definition's package (e.g. `"user"`, `"baml"`)
/// and its short name (e.g. `"Foo"`, `"PrimitiveClient"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub struct QualifiedTypeName {
    /// The package this type is defined in (`Local` for user code, `Dep` for a
    /// dependency like `baml`).
    pkg: Package,
    /// The namespace this type is defined in (e.g. `["llm"]`).
    namespace: Vec<Name>,
    /// The short/local name of the type (e.g. `"Foo"`).
    name: Name,
}

impl QualifiedTypeName {
    pub fn new(pkg: Name, namespace: Vec<Name>, name: Name) -> Self {
        Self {
            pkg: Package::from_name(pkg),
            namespace,
            name,
        }
    }

    /// A local (`user`-package) type with no namespace — a bare class/enum
    /// name. Replaces the legacy `TypeName::local`.
    pub fn local(name: Name) -> Self {
        Self::new(Name::new(RESERVED_USER_PACKAGE), Vec::new(), name)
    }

    /// A runtime-minted local type. The hidden namespace makes the canonical
    /// name unique per mint while user-facing rendering remains the requested
    /// source name. `$dyn` is not a legal user namespace segment, so this
    /// convention cannot collide with a static declaration.
    pub fn runtime_local(name: Name, mint: u64) -> Self {
        Self::new(
            Name::new(RESERVED_USER_PACKAGE),
            vec![Name::new("$dyn"), Name::new(mint.to_string())],
            name,
        )
    }

    pub fn is_runtime_minted(&self) -> bool {
        self.is_local()
            && self
                .namespace
                .first()
                .is_some_and(|segment| segment.as_str() == "$dyn")
    }

    pub fn package(&self) -> &Name {
        self.pkg.as_name()
    }

    /// Whether this type lives in the user's own (implicit root) package — the
    /// "current" package for everything a user writes. User-facing rendering
    /// omits the package for these; only dependency types carry a package
    /// qualifier. Use this instead of comparing `package()` to `"user"`.
    pub fn is_local(&self) -> bool {
        matches!(self.pkg, Package::Local)
    }

    pub fn namespace(&self) -> &Vec<Name> {
        &self.namespace
    }

    pub fn name(&self) -> &Name {
        &self.name
    }

    /// Whether this is the generated stream-view companion for a local type.
    ///
    /// Stream types currently use the `$stream` source-level suffix. Keeping
    /// this query on the compiler-owned qualified name avoids duplicating that
    /// identity rule in each code generator.
    pub fn is_stream(&self) -> bool {
        self.name.as_str().ends_with("$stream")
    }

    /// The unqualified source name, without the generated stream prefix.
    pub fn bare_name(&self) -> &str {
        self.name
            .as_str()
            .strip_suffix("$stream")
            .unwrap_or_else(|| self.name.as_str())
    }

    pub fn is_builtin_root_type(&self, name: &str) -> bool {
        self.package().as_str() == "baml" && self.namespace.is_empty() && self.name.as_str() == name
    }

    /// Returns `true` if this type lives in the `baml.panics` namespace
    /// (i.e. it is a panic class or the `Panic` type alias).
    pub fn is_panic_type(&self) -> bool {
        baml_base::is_panic_namespace(self.package().as_str(), &self.namespace)
    }

    /// The flat `[package, ...namespace]` path, matching the legacy
    /// `TypeName::module_path` representation that fused package and namespace
    /// into one `Vec`. Allocates — prefer [`package`](Self::package) /
    /// [`namespace`](Self::namespace) on hot paths; kept for call sites that
    /// build a fully-qualified dotted string.
    pub fn module_path(&self) -> Vec<Name> {
        std::iter::once(self.pkg.as_name().clone())
            .chain(self.namespace.iter().cloned())
            .collect()
    }

    /// The user-facing display name (legacy `TypeName::display_name`): the
    /// reserved `user` package is elided for local types, dependency packages
    /// are kept.
    pub fn display_name(&self) -> Name {
        if self.is_runtime_minted() {
            return self.name.clone();
        }
        if self.is_local() {
            let parts: Vec<String> = self
                .namespace
                .iter()
                .map(std::string::ToString::to_string)
                .chain(std::iter::once(self.name.to_string()))
                .collect();
            Name::new(parts.join("."))
        } else {
            Name::new(self.to_string())
        }
    }

    /// Parse a dotted path into a qualified name: the first segment is the
    /// package, the last is the short name, and any middle segments form the
    /// namespace (`"baml.json.json"` → pkg `baml`, ns `["json"]`, name `json`).
    /// A single bare segment is treated as a local (`user`-package) type.
    pub fn from_dotted_path(path: &str) -> Self {
        let segments: Vec<&str> = path.split('.').collect();
        let name = Name::new(*segments.last().expect("path must be non-empty"));
        match segments.len() {
            0 | 1 => Self::new(Name::new(RESERVED_USER_PACKAGE), Vec::new(), name),
            _ => Self::new(
                Name::new(segments[0]),
                segments[1..segments.len() - 1]
                    .iter()
                    .map(|s| Name::new(*s))
                    .collect(),
                name,
            ),
        }
    }

    /// The dotted path `package.namespace.name` (no `<generic_params>` suffix).
    /// When `user_facing`, the reserved implicit `user` package is elided
    /// ([`RESERVED_USER_PACKAGE`]) — the single structural source of the
    /// "no `user.` in names" rule. The canonical form (`user_facing = false`)
    /// keeps the package for dumps/identity.
    pub fn render_dotted(&self, user_facing: bool) -> String {
        if user_facing && self.is_runtime_minted() {
            return self.name.to_string();
        }
        let namespace = self
            .namespace
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(".");
        let elide = user_facing && self.is_local();
        let pkg = self.package();
        match (elide, namespace.is_empty()) {
            (true, true) => self.name.to_string(),
            (true, false) => format!("{namespace}.{}", self.name),
            (false, true) => format!("{}.{}", pkg, self.name),
            (false, false) => format!("{}.{namespace}.{}", pkg, self.name),
        }
    }

    /// User-facing rendering of the qualified name: identical to the canonical
    /// [`fmt::Display`] except the reserved implicit `user` package is elided.
    /// Call this instead of post-processing the canonical string.
    pub fn render_user_facing(&self) -> String {
        self.render_dotted(true)
    }

    /// Return the primitive represented by a builtin companion class.
    pub fn builtin_primitive(&self) -> Option<PrimitiveType> {
        if self.package().as_str() != "baml" {
            return None;
        }
        let path: Vec<&str> = self
            .namespace
            .iter()
            .map(Name::as_str)
            .chain(std::iter::once(self.name.as_str()))
            .collect();
        PrimitiveType::from_builtin_class_path(&path)
    }

    /// If this names a builtin `baml` companion class that has a lowercase
    /// primitive/keyword alias, return that alias: `baml.String` → `string`,
    /// `baml.media.Image` → `image`, `baml.json.json` → `json`. Returns `None`
    /// for any other type (including user types and non-aliased `baml` types
    /// such as `baml.json.JsonObject`).
    ///
    /// This is the single collapse rule used by the describe/hover canonical
    /// type printer and delegates to [`BuiltinTypeName`]'s registry.
    pub fn builtin_alias(&self) -> Option<&'static str> {
        if self.package().as_str() != "baml" {
            return None;
        }
        let path: Vec<&str> = self
            .namespace
            .iter()
            .map(Name::as_str)
            .chain(std::iter::once(self.name.as_str()))
            .collect();
        BuiltinTypeName::from_builtin_definition_path(&path).map(BuiltinTypeName::alias)
    }

    /// The addressable spelling: the shortest form that pastes back into
    /// `baml describe` (and name resolution generally) and finds this type
    /// again from any scope. The single source of describe's paste-back
    /// addressing convention:
    ///
    /// - builtin companion class with a lowercase alias → the alias
    ///   (`string`);
    /// - workspace type at package root → its bare name (`Foo`);
    /// - workspace type in a namespace → `root.<ns>.<Name>` (the workspace
    ///   package is addressed as `root` — its literal name would read as an
    ///   item *named* that, which is nothing);
    /// - other dependency type → `<pkg>.<path>` (`baml.json.JsonObject`).
    pub fn render_addressable(&self) -> String {
        if let Some(alias) = self.builtin_alias() {
            return alias.to_string();
        }
        if self.is_local() {
            if self.namespace.is_empty() {
                self.name.to_string()
            } else {
                let path = self
                    .namespace
                    .iter()
                    .chain(std::iter::once(&self.name))
                    .map(Name::as_str)
                    .collect::<Vec<_>>()
                    .join(".");
                format!("{ADDRESSABLE_USER_PACKAGE}.{path}")
            }
        } else {
            self.render_user_facing()
        }
    }
}

/// How the workspace package is spelled in addressable paths (`root.ns.Foo`):
/// the counterpart of [`RESERVED_USER_PACKAGE`] for paste-back output. The
/// literal package name would read as an item named `user`, which is nothing.
pub const ADDRESSABLE_USER_PACKAGE: &str = "root";

/// The package-name prefix for addressable paths: the workspace package is
/// spelled [`ADDRESSABLE_USER_PACKAGE`], every other package by its own name.
pub fn addressable_package(package: &Name) -> &str {
    if package.as_str() == RESERVED_USER_PACKAGE {
        ADDRESSABLE_USER_PACKAGE
    } else {
        package.as_str()
    }
}

/// The reserved implicit root package for user-authored code. It is the
/// *current* package for everything a user writes, so it must never be shown in
/// user-facing output (`user.Dog` → `Dog`). The canonical `Display` keeps it
/// (for dumps/identity); only the user-facing path elides it.
pub const RESERVED_USER_PACKAGE: &str = "user";

/// Prefix of synthetic effect-polymorphism type parameters. These are an
/// internal encoding (`__effect_param_0`, …); user-facing rendering shows them
/// as `callback`. Single source of truth — use [`is_synthetic_effect_param`]
/// rather than re-deriving this prefix check.
pub const SYNTHETIC_EFFECT_PARAM_PREFIX: &str = "__effect_param_";

/// Whether `name` is a synthesized effect-polymorphism type parameter
/// (`__effect_param_N`). The single source of truth for this check — TIR, MIR,
/// and the LSP all call here instead of re-implementing the prefix match.
pub fn is_synthetic_effect_param(name: &Name) -> bool {
    name.as_str()
        .strip_prefix(SYNTHETIC_EFFECT_PARAM_PREFIX)
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
}

impl fmt::Display for QualifiedTypeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.render_dotted(false))
    }
}

#[cfg(test)]
mod alias_tests {
    use baml_base::Name;

    use crate::{PrimitiveType, QualifiedTypeName};

    #[test]
    fn primitive_alias_class_path_roundtrips() {
        for p in PrimitiveType::ALL {
            let path = p.builtin_class_path();
            assert_eq!(
                PrimitiveType::from_builtin_class_path(path),
                Some(p),
                "round-trip failed for {p:?} via {path:?}"
            );
            // The alias matches the Display spelling.
            assert_eq!(p.alias(), p.to_string());
        }
    }

    #[test]
    fn from_builtin_class_path_rejects_unknown() {
        assert_eq!(PrimitiveType::from_builtin_class_path(&["Nope"]), None);
        assert_eq!(
            PrimitiveType::from_builtin_class_path(&["media", "Nope"]),
            None
        );
    }

    fn baml_qtn(namespace: &[&str], name: &str) -> QualifiedTypeName {
        QualifiedTypeName::new(
            Name::new("baml"),
            namespace.iter().copied().map(Name::new).collect(),
            Name::new(name),
        )
    }

    #[test]
    fn builtin_alias_collapses_primitive_classes() {
        assert_eq!(baml_qtn(&[], "String").builtin_alias(), Some("string"));
        assert_eq!(baml_qtn(&[], "Int").builtin_alias(), Some("int"));
        assert_eq!(baml_qtn(&["media"], "Image").builtin_alias(), Some("image"));
        assert_eq!(baml_qtn(&["media"], "Pdf").builtin_alias(), Some("pdf"));
    }

    #[test]
    fn builtin_primitive_recognizes_companion_classes() {
        assert_eq!(
            baml_qtn(&[], "String").builtin_primitive(),
            Some(PrimitiveType::String)
        );
        assert_eq!(
            baml_qtn(&["media"], "Image").builtin_primitive(),
            Some(PrimitiveType::Image)
        );
        assert_eq!(baml_qtn(&["json"], "json").builtin_primitive(), None);
    }

    #[test]
    fn builtin_alias_handles_json_special_case() {
        // `json` is the `baml.json.json` type alias, not a `PrimitiveType`.
        assert_eq!(baml_qtn(&["json"], "json").builtin_alias(), Some("json"));
        // A non-aliased `baml.json` type collapses to nothing.
        assert_eq!(baml_qtn(&["json"], "JsonObject").builtin_alias(), None);
    }

    #[test]
    fn builtin_alias_ignores_user_and_unaliased_types() {
        // User-package `String` is never collapsed.
        let user = QualifiedTypeName::new(Name::new("user"), vec![], Name::new("String"));
        assert_eq!(user.builtin_alias(), None);
        // A `baml` class without a primitive alias is not collapsed.
        assert_eq!(baml_qtn(&[], "SomethingElse").builtin_alias(), None);
    }
}
