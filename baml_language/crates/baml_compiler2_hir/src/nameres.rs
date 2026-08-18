//! Path resolution: THE resolution chain, defined once.
//!
//! The analog of rust-analyzer's `hir-def` path resolution
//! (`nameres/path_resolution.rs`): one ordered walk that every consumer -
//! type lowering in `baml_compiler2_hir_ty`, stream expansion in
//! `baml_compiler2_ppir` - goes through, so no two subsystems can disagree
//! about what a written name denotes.
//!
//! ```text
//! builtin scope -> local scope -> qualified (root. / package)
//!   -> baml namespace shorthand -> $stream companion base
//! ```
//!
//! The chain's ORDER and mechanics live here. Two steps are injected through
//! [`ForeignLookup`], because they are legitimately phase-specific:
//!
//! * which symbol tables a *foreign package* name consults - type lowering
//!   resolves against post-expansion tables with mounted-interface access
//!   control, stream expansion against pre-expansion tables (it runs while
//!   producing the post-expansion ones);
//! * the same choice for the `baml` shorthand layer's tables and visibility.
//!
//! This mirrors rust-analyzer, where the resolver's extern-prelude and
//! prelude steps are database-injected while the walk itself is fixed.
//!
//! Builtin type names are a LAYER of this chain, not a decision taken before
//! it: `string` resolves to [`TypePathResolution::Builtin`] the way a path
//! resolves to a definition (rust-analyzer's `TypeNs::BuiltinType`), so a
//! user declaration that shadows a builtin spelling is visible to the
//! resolver instead of being unreachable, and stays addressable as
//! `root.<name>`.

use baml_base::Name;
use baml_type::BuiltinTypeName;

use crate::{contributions::Definition, package::PackageItems};

/// The builtin type scope: BAML's analog of rust-analyzer's `BUILTIN_SCOPE`
/// (`hir-def/src/item_scope.rs`).
///
/// Only names with an addressable definition are in scope. `void`, `never`,
/// and `unknown` are compiler intrinsics - `builtin_definition_path` returns
/// `None` for them - so they have no companion class to be confused with and
/// keep their syntactic validation (`void` is legal only as a bare return
/// type, checked before resolution runs).
pub fn builtin_type_scope(name: &Name) -> Option<BuiltinTypeName> {
    BuiltinTypeName::from_alias(name.as_str())
        .filter(|builtin| builtin.builtin_definition_path().is_some())
}

/// What a type path resolves to - rust-analyzer's `TypeNs`, parameterized
/// over the consumer's representation of a foreign-package hit (`X`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypePathResolution<'db, X> {
    /// A builtin-scope hit: `string`, `int`, `json`. Builtin-ness is a
    /// RESOLUTION RESULT, not a syntax-node kind decided before any scope
    /// exists.
    Builtin(BuiltinTypeName),
    /// A declaration in this package or a dependency's raw tables.
    Def(Definition<'db>),
    /// A foreign-package hit in the consumer's own representation (type
    /// lowering's exported-interface view; unused by consumers whose foreign
    /// tables are plain `Definition`s).
    Foreign(X),
}

/// The two phase-specific steps of the chain: how a foreign package's tables
/// are consulted, and the same choice for the `baml` shorthand layer. The
/// chain decides WHEN these fire; implementations decide against WHAT.
pub trait ForeignLookup<'db> {
    /// The consumer's representation of a foreign hit that is not a plain
    /// `Definition` (use [`std::convert::Infallible`] when there is none).
    type Res;

    /// A type item in the package named `package`. `None` lets the chain
    /// proceed to its later layers.
    fn lookup_type(
        &self,
        package: &Name,
        namespace: &[Name],
        item: &Name,
    ) -> Option<TypePathResolution<'db, Self::Res>>;

    /// A value item in the package named `package`.
    fn lookup_value(
        &self,
        package: &Name,
        namespace: &[Name],
        item: &Name,
    ) -> Option<Definition<'db>>;

    /// The `baml` shorthand layer's type lookup: `namespace` is the full
    /// written prefix (`reflect.class` for `reflect.class.Type`),
    /// reinterpreted under the builtin `baml` package. Implementations own
    /// the visibility policy.
    fn baml_shorthand_type(
        &self,
        namespace: &[Name],
        item: &Name,
    ) -> Option<TypePathResolution<'db, Self::Res>>;

    /// The `baml` shorthand layer's value lookup.
    fn baml_shorthand_value(&self, namespace: &[Name], item: &Name) -> Option<Definition<'db>>;

    /// Whether this foreign hit may serve as a `$stream` companion base
    /// (classes and type aliases have companions; nothing else does).
    fn is_stream_base(res: &Self::Res) -> bool;
}

/// Heads the shorthand layer reinterprets under the `baml` package (BEP-066).
fn is_shorthand_head(name: &Name) -> bool {
    matches!(name.as_str(), "reflect" | "type" | "json")
}

/// The resolver: the chain's mechanics over phase-supplied symbol tables.
pub struct Resolver<'a, 'db, F> {
    /// This package's tables - the phase decides which generation
    /// (pre- or post-`$stream`-expansion).
    pub package_items: &'a PackageItems<'db>,
    /// The namespace the resolved path is written in.
    pub ns_context: &'a [Name],
    /// The phase-specific steps.
    pub foreign: F,
}

impl<'db, F: ForeignLookup<'db>> Resolver<'_, 'db, F> {
    /// Resolve a written TYPE path through the chain.
    pub fn resolve_type_path(&self, segments: &[Name]) -> Option<TypePathResolution<'db, F::Res>> {
        // Layer 0: the builtin scope. Only an UNQUALIFIED name can mean a
        // builtin - `root.string` and `pkg.string` explicitly ask for a
        // declaration. The builtin outranks the local scope: a builtin name
        // means the builtin in every type position, and the declaration it
        // shadows stays addressable as `root.<name>`. (rust-analyzer needs a
        // `BuiltinShadowMode` flag here because Rust really is ambiguous - a
        // `mod u8` and the primitive share the type namespace; BAML is not.)
        if let [single] = segments
            && let Some(builtin) = builtin_type_scope(single)
        {
            return Some(TypePathResolution::Builtin(builtin));
        }

        // Layer 1: the current namespace, then the rest of this package.
        let (item, seg_ns) = segments.split_last().expect("type paths are never empty");
        let relative_ns: Vec<Name> = if self.ns_context.is_empty() {
            seg_ns.to_vec()
        } else {
            self.ns_context.iter().chain(seg_ns).cloned().collect()
        };
        if let Some(def) = self.package_items.lookup_type(&relative_ns, item) {
            return Some(TypePathResolution::Def(def));
        }

        // Layer 2: an explicitly qualified path - `root.`-absolute within
        // this package, or rooted at another package's name.
        if segments.len() >= 2 {
            let prefix_ns = &segments[1..segments.len() - 1];
            if segments[0].as_str() == "root" {
                if let Some(def) = self.package_items.lookup_type(prefix_ns, item) {
                    return Some(TypePathResolution::Def(def));
                }
            } else if let Some(resolved) = self.foreign.lookup_type(&segments[0], prefix_ns, item) {
                return Some(resolved);
            }
        }

        // Layer 3: the BEP-066 namespace shorthand - `reflect.*`, `type.*`,
        // and `json.*` reinterpreted under the builtin `baml` package.
        // Ordered after the local scope, so a user namespace of the same
        // name shadows it (the way a local `mod` shadows an extern crate in
        // Rust); `baml.` is the disambiguating qualifier.
        if segments.first().is_some_and(is_shorthand_head)
            && let Some(resolved) = self
                .foreign
                .baml_shorthand_type(&segments[..segments.len() - 1], item)
        {
            return Some(resolved);
        }

        // Layer 4: `$stream` companions of classes/aliases resolve through
        // their base name; the caller re-qualifies under the `$stream` name.
        if let Some(base) = item.as_str().strip_suffix("$stream") {
            let mut base_segments = segments.to_vec();
            *base_segments.last_mut().expect("non-empty") = Name::new(base);
            return self
                .resolve_type_path(&base_segments)
                .filter(|resolved| match resolved {
                    TypePathResolution::Def(Definition::Class(_) | Definition::TypeAlias(_)) => {
                        true
                    }
                    TypePathResolution::Foreign(res) => F::is_stream_base(res),
                    TypePathResolution::Def(_) | TypePathResolution::Builtin(_) => false,
                });
        }

        None
    }

    /// Resolve a written VALUE path through the chain: the same walk over
    /// `lookup_value`, minus two layers - there is no builtin VALUE scope
    /// (no BAML value has a bare name that is not an ordinary item;
    /// rust-analyzer's `BUILTIN_SCOPE` is likewise types-only, with
    /// `Vec`/`drop` coming from the ordinary std prelude), and no `$stream`
    /// fallback (companions are functions with their own names).
    pub fn resolve_value_path(&self, segments: &[Name]) -> Option<Definition<'db>> {
        // Layer 1: the current namespace, then the rest of this package.
        let (item, seg_ns) = segments.split_last()?;
        let relative_ns: Vec<Name> = if self.ns_context.is_empty() {
            seg_ns.to_vec()
        } else {
            self.ns_context.iter().chain(seg_ns).cloned().collect()
        };
        if let Some(def) = self.package_items.lookup_value(&relative_ns, item) {
            return Some(def);
        }

        // Layer 2: `root.`-absolute or package-rooted.
        if segments.len() >= 2 {
            let prefix_ns = &segments[1..segments.len() - 1];
            if segments[0].as_str() == "root" {
                if let Some(def) = self.package_items.lookup_value(prefix_ns, item) {
                    return Some(def);
                }
            } else if let Some(def) = self.foreign.lookup_value(&segments[0], prefix_ns, item) {
                return Some(def);
            }
        }

        // Layer 3: the `baml` shorthand.
        if segments.first().is_some_and(is_shorthand_head)
            && let Some(def) = self
                .foreign
                .baml_shorthand_value(&segments[..segments.len() - 1], item)
        {
            return Some(def);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The builtin scope must cover the whole registry, minus the
    /// intrinsics. `PrimitiveType::ALL` drives the loop, so adding a
    /// primitive to `baml_type` fails here until the scope admits it.
    #[test]
    fn builtin_scope_covers_every_registry_alias() {
        for primitive in baml_type::PrimitiveType::ALL {
            let alias = Name::new(primitive.alias());
            assert_eq!(
                builtin_type_scope(&alias),
                Some(BuiltinTypeName::Primitive(primitive)),
                "primitive alias `{}` is missing from the builtin scope",
                primitive.alias()
            );
        }
        assert_eq!(
            builtin_type_scope(&Name::new("json")),
            Some(BuiltinTypeName::Json),
        );
    }

    /// Intrinsics stay OUT of the scope: they have no addressable
    /// definition, so they cannot collide with a declaration, and they are
    /// validated as syntax before resolution runs.
    #[test]
    fn builtin_scope_excludes_definitionless_intrinsics() {
        for intrinsic in ["void", "never", "unknown"] {
            assert_eq!(
                builtin_type_scope(&Name::new(intrinsic)),
                None,
                "`{intrinsic}` is an intrinsic and must not be in the builtin scope",
            );
            assert!(
                BuiltinTypeName::from_alias(intrinsic).is_some(),
                "`{intrinsic}` should still be a registry entry",
            );
        }
    }

    /// An ordinary name is not a builtin, and neither is a capitalized
    /// companion-class name: `baml.String` is reached as a path, not through
    /// the builtin scope, so `class String` in user code is an ordinary
    /// same-package declaration rather than a builtin shadow.
    #[test]
    fn builtin_scope_is_alias_spelled_only() {
        assert_eq!(builtin_type_scope(&Name::new("Dog")), None);
        assert_eq!(builtin_type_scope(&Name::new("String")), None);
        assert_eq!(builtin_type_scope(&Name::new("Int")), None);
    }
}
