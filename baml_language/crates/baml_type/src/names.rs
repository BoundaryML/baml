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
            Self::runtime_namespace(mint, &[]),
            name,
        )
    }

    /// [`runtime_local`](Self::runtime_local) for a name that already lives in
    /// a namespace: the hidden `$dyn.<mint>` segments are prepended, so a
    /// runtime-compiled package's `ns.Item` becomes `user.$dyn.7.ns.Item` and
    /// keeps its own namespace structure below the discriminator.
    ///
    /// `None` when `self` is not a plain local name — a dependency type, or one
    /// that is already runtime-minted, has an identity that is not this
    /// package's to reassign.
    pub fn to_runtime_local(&self, mint: u64) -> Option<Self> {
        (self.is_local() && !self.is_runtime_minted()).then(|| Self {
            pkg: Package::Local,
            namespace: Self::runtime_namespace(mint, &self.namespace),
            name: self.name.clone(),
        })
    }

    /// The hidden namespace a runtime mint contributes, ahead of `rest`.
    fn runtime_namespace(mint: u64, rest: &[Name]) -> Vec<Name> {
        std::iter::once(Name::new(RUNTIME_MINT_NAMESPACE))
            .chain(std::iter::once(Name::new(mint.to_string())))
            .chain(rest.iter().cloned())
            .collect()
    }

    pub fn is_runtime_minted(&self) -> bool {
        self.is_local()
            && self
                .namespace
                .first()
                .is_some_and(|segment| segment.as_str() == RUNTIME_MINT_NAMESPACE)
    }

    /// Whether this name was minted by `mint` — the discriminator check that
    /// keeps one runtime package's declarations from answering for another's.
    pub fn has_runtime_mint(&self, mint: u64) -> bool {
        self.is_runtime_minted()
            && self
                .namespace
                .get(1)
                .is_some_and(|segment| segment.as_str().parse::<u64>() == Ok(mint))
    }

    /// The namespace below the hidden runtime-mint discriminator — what the
    /// name would have been had it never been minted. Empty for a name that is
    /// minted at the package root, and the whole namespace for one that is not
    /// minted at all. Lookups keyed on a *source-visible* namespace (a package's
    /// own `LocalName` tables) go through this rather than `namespace()`.
    pub fn source_namespace(&self) -> &[Name] {
        if self.is_runtime_minted() {
            // `get` rather than a slice: `from_dotted_path` will happily build a
            // truncated `user.$dyn.Foo` out of an untrusted string, and a name
            // that cannot be one this crate minted must render, not panic.
            self.namespace.get(2..).unwrap_or_default()
        } else {
            &self.namespace
        }
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
        if self.is_local() {
            let parts: Vec<String> = self
                .source_namespace()
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
        // A runtime mint is an identity token, never a spelling: user-facing
        // rendering shows the name the source asked for, below the hidden
        // discriminator.
        let segments = if user_facing {
            self.source_namespace()
        } else {
            &self.namespace
        };
        self.dotted(segments, user_facing && self.is_local())
    }

    /// Join `[package.]segments.name`, eliding the package when asked. The one
    /// place the dotted spelling is assembled — every renderer picks which
    /// namespace segments to show and whether the package is elided, and shares
    /// this.
    fn dotted(&self, segments: &[Name], elide_package: bool) -> String {
        let namespace = segments
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(".");
        let pkg = self.package();
        match (elide_package, namespace.is_empty()) {
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

    /// The package-qualified spelling with the runtime mint elided: a minted
    /// `user.$dyn.7.Item` renders `user.Item`, and every unminted name renders
    /// exactly as [`fmt::Display`] does.
    ///
    /// A mint is an identity token, never a spelling. Surfaces that print a
    /// package-qualified name to somebody outside the VM — a coercion or decode
    /// error, a host SDK's `class_name`, a trace value — go through this so a
    /// minted declaration reads the way the same declaration read before it was
    /// minted. `Display` keeps the discriminator, because dumps and identity
    /// comparisons are the one audience that needs to tell two `Item`s apart.
    pub fn render_source_dotted(&self) -> String {
        self.source_spelling().to_string()
    }

    /// [`render_source_dotted`](Self::render_source_dotted) as a borrowing
    /// [`fmt::Display`] adapter, so an error message can interpolate it
    /// (`format!("class `{}` not found", qtn.source_spelling())`) without
    /// building a `String` first.
    pub fn source_spelling(&self) -> SourceSpelling<'_> {
        SourceSpelling(self)
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
}

/// The reserved implicit root package for user-authored code. It is the
/// *current* package for everything a user writes, so it must never be shown in
/// user-facing output (`user.Dog` → `Dog`). The canonical `Display` keeps it
/// (for dumps/identity); only the user-facing path elides it.
pub const RESERVED_USER_PACKAGE: &str = "user";

/// The hidden namespace segment that marks a runtime-minted name. It is not a
/// legal user namespace segment, so a minted name can never collide with a
/// statically declared one. The segment after it is the mint that made the
/// name unique. Single source of truth for the convention —
/// [`QualifiedTypeName::runtime_local`] writes it and
/// [`QualifiedTypeName::is_runtime_minted`] reads it.
pub const RUNTIME_MINT_NAMESPACE: &str = "$dyn";

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

/// [`fmt::Display`] adapter for [`QualifiedTypeName::source_spelling`]: the
/// package-qualified name with the runtime mint elided.
pub struct SourceSpelling<'a>(&'a QualifiedTypeName);

impl fmt::Display for SourceSpelling<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.dotted(self.0.source_namespace(), false))
    }
}

#[cfg(test)]
mod runtime_mint_tests {
    use baml_base::Name;

    use crate::QualifiedTypeName;

    fn local_ns(namespace: &[&str], name: &str) -> QualifiedTypeName {
        QualifiedTypeName::new(
            Name::new(super::RESERVED_USER_PACKAGE),
            namespace.iter().copied().map(Name::new).collect(),
            Name::new(name),
        )
    }

    #[test]
    fn minting_keeps_the_source_namespace_below_the_discriminator() {
        let minted = local_ns(&["models"], "Item")
            .to_runtime_local(7)
            .expect("a plain local name can be minted");
        assert!(minted.is_runtime_minted());
        assert!(minted.has_runtime_mint(7));
        assert!(!minted.has_runtime_mint(8));
        assert_eq!(minted.source_namespace(), [Name::new("models")]);
        // Canonical form keys the definition; every rendered form is the
        // spelling the source wrote.
        assert_eq!(minted.to_string(), "user.$dyn.7.models.Item");
        assert_eq!(minted.render_user_facing(), "models.Item");
        assert_eq!(minted.display_name().as_str(), "models.Item");
        assert_eq!(minted.render_source_dotted(), "user.models.Item");
    }

    /// The package-qualified surfaces (a host SDK's `class_name`, a coercion or
    /// decode error) must read exactly as they read before the name was minted.
    #[test]
    fn the_source_spelling_matches_the_unminted_name() {
        for plain in [local_ns(&[], "Item"), local_ns(&["models"], "Item")] {
            let minted = plain.to_runtime_local(7).expect("mintable");
            assert_eq!(minted.render_source_dotted(), plain.to_string());
            assert_eq!(minted.source_spelling().to_string(), plain.to_string());
            assert_eq!(minted.render_user_facing(), plain.render_user_facing());
        }
    }

    /// An unminted name renders identically through both spellings, so a call
    /// site can mask unconditionally without a `is_runtime_minted` guard.
    #[test]
    fn an_unminted_name_renders_the_same_either_way() {
        for name in [
            local_ns(&[], "Item"),
            local_ns(&["models"], "Item"),
            QualifiedTypeName::new(
                Name::new("baml"),
                vec![Name::new("json")],
                Name::new("json"),
            ),
        ] {
            assert_eq!(name.render_source_dotted(), name.to_string());
        }
    }

    #[test]
    fn two_mints_of_one_name_are_distinct_keys() {
        let name = local_ns(&[], "Item");
        let first = name.to_runtime_local(1).expect("mintable");
        let second = name.to_runtime_local(2).expect("mintable");
        assert_ne!(first, second);
        assert_eq!(first.render_user_facing(), second.render_user_facing());
        // Neither collides with the plain declaration they were spelled from.
        assert_ne!(first, name);
        assert_ne!(second, name);
    }

    #[test]
    fn a_dependency_or_already_minted_name_is_not_reminted() {
        let dependency = QualifiedTypeName::new(Name::new("baml"), vec![], Name::new("Item"));
        assert_eq!(dependency.to_runtime_local(7), None);
        let minted = local_ns(&[], "Item").to_runtime_local(7).expect("mintable");
        assert_eq!(minted.to_runtime_local(9), None);
    }

    /// `from_dotted_path` accepts any dotted string, including a truncated one
    /// that looks minted but carries no mint. It must still render.
    #[test]
    fn a_truncated_mint_namespace_does_not_panic() {
        let truncated = QualifiedTypeName::from_dotted_path("user.$dyn.Item");
        assert!(truncated.is_runtime_minted());
        assert!(!truncated.has_runtime_mint(7));
        assert!(truncated.source_namespace().is_empty());
        assert_eq!(truncated.render_user_facing(), "Item");
    }

    #[test]
    fn a_plain_name_reports_its_whole_namespace_as_source() {
        let plain = local_ns(&["models"], "Item");
        assert!(!plain.is_runtime_minted());
        assert!(!plain.has_runtime_mint(7));
        assert_eq!(plain.source_namespace(), plain.namespace().as_slice());
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
