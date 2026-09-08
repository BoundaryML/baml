//! Qualified type names and package keys.
//!
//! [`QualifiedTypeName`] identifies a class/enum/interface/type-alias by the
//! package it is declared in, its namespace path, and its short name. It is
//! generic over the *package key* `P`, because the two layers that name a
//! declaration identify its package differently:
//!
//! - The compiler keys by the package itself: [`DeclName`] carries the
//!   [`SourceRoot`] (a package IS a root; see `baml_base::files`). Two
//!   consumers that spell one package differently see EQUAL heads, a
//!   nameless package needs no name, and nothing can be rendered without a
//!   viewpoint — a `DeclName` has no `Display`, no `Borsh`, and no
//!   `HeadDisplay`, so every db-free spelling of a compiler type is a compile
//!   error rather than a leak.
//! - The wire keys by spelling: [`TypeName`](crate::TypeName) carries a
//!   [`Package`], the artifact's own root (`Local`) or a dependency by the name
//!   the artifact's edges give it (`Dep`). An artifact is its own viewpoint, so
//!   this is edge-relative naming by construction (rustc's `LOCAL_CRATE`).
//!
//! The emit boundary maps the first to the second through the emitting
//! package's dependency edges; the import boundary (a package interface blob)
//! maps back through the importing root's edges. Nothing in between compares
//! a package name string.

use std::fmt;

use baml_base::{LangPackage, LangRoots, Name, SourceRoot};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::{BuiltinTypeName, PrimitiveType};

/// The wire's package key: which package a type is defined in, relative to the
/// artifact naming it. `Local` is the artifact's own root — its spelling is a
/// display decision (the root's declared name, else the default
/// [`RESERVED_USER_PACKAGE`]); `Dep(name)` is a dependency by the name the
/// artifact's dependency edge gives it (e.g. `baml`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum Package {
    /// The artifact's own root.
    Local,
    /// A dependency, by the artifact's edge name for it.
    Dep(Name),
}

/// The interned `Name` of the default self-spelling, materialized once so
/// [`QualifiedTypeName::package`] can hand out a `&Name` for `Local`.
static USER_PACKAGE_NAME: Name = Name::new_inline(RESERVED_USER_PACKAGE);

impl Package {
    /// The wire codec from a spelling: the default self-spelling
    /// [`RESERVED_USER_PACKAGE`] encodes as [`Package::Local`], any other
    /// spelling as [`Package::Dep`]. Only the emit boundary (a root's closure
    /// spelling → its wire key) and wire-side reparses of a rendered name go
    /// through here; the compiler's own heads never carry a spelling.
    pub fn from_name(name: Name) -> Self {
        if name.as_str() == RESERVED_USER_PACKAGE {
            Package::Local
        } else {
            Package::Dep(name)
        }
    }

    /// The package's spelling (`Local` spells as the default self name).
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

/// A qualified type name: a declaration's package (by key `P`), namespace
/// path, and short name.
///
/// Used in `Ty::Class`, `Ty::Enum`, `Ty::Interface`, and `Ty::TypeAlias` to
/// unambiguously identify a declaration. See the module docs for the two
/// package keys.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub struct QualifiedTypeName<P = Package> {
    /// The package this type is defined in, by key.
    pkg: P,
    /// The namespace this type is defined in (e.g. `["llm"]`).
    namespace: Vec<Name>,
    /// The short/local name of the type (e.g. `"Foo"`).
    name: Name,
}

/// The compiler's qualified name: the declaring package IS its source root.
/// Session-local — never rendered without a viewpoint, never serialized.
pub type DeclName = QualifiedTypeName<SourceRoot>;

impl<P> QualifiedTypeName<P> {
    /// A qualified name under package key `pkg`.
    pub fn qualified(pkg: P, namespace: Vec<Name>, name: Name) -> Self {
        Self {
            pkg,
            namespace,
            name,
        }
    }

    /// The package key.
    pub fn key(&self) -> &P {
        &self.pkg
    }

    pub fn namespace(&self) -> &Vec<Name> {
        &self.namespace
    }

    pub fn name(&self) -> &Name {
        &self.name
    }

    /// The same declaration under a different package key: the boundary
    /// operation that re-spells a head (root → wire name, or wire name →
    /// root).
    pub fn map_key<Q>(&self, f: impl FnOnce(&P) -> Q) -> QualifiedTypeName<Q>
    where
        Name: Clone,
    {
        QualifiedTypeName {
            pkg: f(&self.pkg),
            namespace: self.namespace.clone(),
            name: self.name.clone(),
        }
    }

    /// [`map_key`](Self::map_key) for a fallible re-spelling.
    pub fn try_map_key<Q, E>(
        &self,
        f: impl FnOnce(&P) -> Result<Q, E>,
    ) -> Result<QualifiedTypeName<Q>, E> {
        Ok(QualifiedTypeName {
            pkg: f(&self.pkg)?,
            namespace: self.namespace.clone(),
            name: self.name.clone(),
        })
    }

    /// Whether this is the generated stream-view companion for a type.
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

    /// The `[...namespace, name]` path inside the package, as borrowed
    /// strings — the builtin registries are keyed this way.
    fn path_in_package(&self) -> Vec<&str> {
        self.namespace
            .iter()
            .map(Name::as_str)
            .chain(std::iter::once(self.name.as_str()))
            .collect()
    }

    /// Whether this names `name` at the root namespace of `pkg`'s declaring
    /// package — the shape every builtin-recognition predicate shares.
    fn is_root_item(&self, name: &str) -> bool {
        self.namespace.is_empty() && self.name.as_str() == name
    }
}

impl DeclName {
    /// A declaration in `root`'s package.
    pub fn in_root(root: SourceRoot, namespace: Vec<Name>, name: Name) -> Self {
        Self::qualified(root, namespace, name)
    }

    /// The declaring package.
    pub fn root(&self) -> SourceRoot {
        self.pkg
    }

    /// Whether this is `name` at the root namespace of the installed language
    /// package `package` (`baml.Int`, `reflect.AnyClass`, …). `false` when
    /// that package is not installed.
    pub fn is_lang_root_type(&self, lang: LangRoots, package: LangPackage, name: &str) -> bool {
        lang.is(package, self.pkg) && self.is_root_item(name)
    }

    /// Whether this lives in the `baml.panics` namespace (a panic class or the
    /// `Panic` alias).
    pub fn is_panic_type(&self, lang: LangRoots) -> bool {
        lang.is(LangPackage::Baml, self.pkg)
            && self.namespace.len() == 1
            && self.namespace[0].as_str() == baml_base::PANICS_NAMESPACE
    }

    /// The primitive a builtin companion class stands for
    /// (`baml.Int` → `int`), if this is one.
    pub fn builtin_primitive(&self, lang: LangRoots) -> Option<PrimitiveType> {
        lang.is(LangPackage::Baml, self.pkg)
            .then(|| PrimitiveType::from_builtin_class_path(&self.path_in_package()))
            .flatten()
    }

    /// The lowercase alias of a builtin companion class (`baml.String` →
    /// `string`, `baml.media.Image` → `image`, `baml.json.json` → `json`), if
    /// this is one. See [`TypeName::builtin_alias`](QualifiedTypeName::builtin_alias).
    pub fn builtin_alias(&self, lang: LangRoots) -> Option<&'static str> {
        lang.is(LangPackage::Baml, self.pkg)
            .then(|| {
                BuiltinTypeName::from_builtin_definition_path(&self.path_in_package())
                    .map(BuiltinTypeName::alias)
            })
            .flatten()
    }
}

impl QualifiedTypeName<Package> {
    /// A wire name from a package spelling (see [`Package::from_name`]).
    pub fn new(pkg: Name, namespace: Vec<Name>, name: Name) -> Self {
        Self {
            pkg: Package::from_name(pkg),
            namespace,
            name,
        }
    }

    /// A type at the root namespace of the artifact's own package — a bare
    /// class/enum name.
    pub fn local(name: Name) -> Self {
        Self::qualified(Package::Local, Vec::new(), name)
    }

    /// The package spelling (`Local` spells as [`RESERVED_USER_PACKAGE`]).
    pub fn package(&self) -> &Name {
        self.pkg.as_name()
    }

    /// Whether this type lives in the artifact's own package. Wire-side
    /// rendering omits the package for these; only dependency types carry a
    /// package qualifier. Use this instead of comparing `package()` to the
    /// default spelling.
    pub fn is_local(&self) -> bool {
        matches!(self.pkg, Package::Local)
    }

    pub fn is_builtin_root_type(&self, name: &str) -> bool {
        self.package().as_str() == "baml" && self.is_root_item(name)
    }

    /// [`Self::is_builtin_root_type`] for the `reflect` package's root
    /// namespace (`reflect.AnyFunction`, `reflect.AnyClass`, …).
    pub fn is_reflect_root_type(&self, name: &str) -> bool {
        self.package().as_str() == "reflect" && self.is_root_item(name)
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
    /// artifact's own package is elided, dependency packages are kept.
    pub fn display_name(&self) -> Name {
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
    /// A single bare segment is treated as a type of the artifact's own package.
    pub fn from_dotted_path(path: &str) -> Self {
        let segments: Vec<&str> = path.split('.').collect();
        let name = Name::new(*segments.last().expect("path must be non-empty"));
        match segments.len() {
            0 | 1 => Self::local(name),
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
    /// When `user_facing`, the artifact's own package is elided — the single
    /// structural source of the "no `user.` in names" rule. The canonical
    /// form (`user_facing = false`) keeps the package for dumps/identity.
    pub fn render_dotted(&self, user_facing: bool) -> String {
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
    /// [`fmt::Display`] except the artifact's own package is elided. Call this
    /// instead of post-processing the canonical string.
    pub fn render_user_facing(&self) -> String {
        self.render_dotted(true)
    }

    /// Return the primitive represented by a builtin companion class.
    pub fn builtin_primitive(&self) -> Option<PrimitiveType> {
        if self.package().as_str() != "baml" {
            return None;
        }
        PrimitiveType::from_builtin_class_path(&self.path_in_package())
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
        BuiltinTypeName::from_builtin_definition_path(&self.path_in_package())
            .map(BuiltinTypeName::alias)
    }
}

/// How the workspace package is spelled in addressable paths (`root.ns.Foo`):
/// the counterpart of [`RESERVED_USER_PACKAGE`] for paste-back output. The
/// literal package name would read as an item named `user`, which is nothing.
pub const ADDRESSABLE_USER_PACKAGE: &str = "root";

/// The default display spelling of a nameless package that nothing depends
/// on — a project with no manifest, a runtime-compiled package. It has no
/// semantics: it is what such a package's own types are spelled by on the
/// wire ([`Package::Local`]) and never shown in user-facing output
/// (`user.Dog` → `Dog`). No code path classifies a package by it.
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

impl fmt::Display for QualifiedTypeName<Package> {
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
