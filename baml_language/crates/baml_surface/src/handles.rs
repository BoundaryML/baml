//! Handles: `Copy` ids over the compiler's interned item locs, with an
//! object-flavored method surface.
//!
//! Every property is a thin wrapper over exactly one query — PPIR `item_data`
//! for the syntactic surface (names, docstrings, spans, membership), and
//! [`crate::facts`] for anything type-resolved. A handle carries no data:
//! two handles are equal iff they name the same item, and everything else is
//! asked of the database at use time.

use baml_base::{Name, SourceFile};
use baml_compiler2_ast::{FunctionOrigin, LetOrigin};
use baml_compiler2_hir::{
    contributions::Definition,
    loc::{
        ClassLoc, ClientLoc, EnumLoc, FunctionLoc, ImplLoc, InterfaceLoc, LetLoc, RetryPolicyLoc,
        TemplateStringLoc, TestLoc, TypeAliasLoc,
    },
    namespace::NamespaceId,
    package::PackageId,
};
use baml_compiler2_ppir::item_data;
use baml_type::{Interface as InterfaceBound, ParamTy, Ty};
use text_size::TextRange;

use crate::{Db, facts};

// ── Handle definitions ────────────────────────────────────────────────────────

/// Declare a `Copy` handle wrapping an interned loc, with `From` conversions
/// both ways and a `file` accessor.
macro_rules! handle {
    ($(#[$doc:meta])* $name:ident, $loc:ty) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name<'db>(pub(crate) $loc);

        impl<'db> $name<'db> {
            /// The underlying compiler loc. An escape hatch for callers mid-
            /// migration; new code should stay on handle methods.
            pub fn loc(self) -> $loc {
                self.0
            }

            /// The file this item is declared in.
            pub fn file(self, db: &'db dyn Db) -> SourceFile {
                self.0.file(db)
            }
        }

        impl<'db> From<$loc> for $name<'db> {
            fn from(loc: $loc) -> Self {
                Self(loc)
            }
        }

        impl<'db> From<$name<'db>> for $loc {
            fn from(handle: $name<'db>) -> Self {
                handle.0
            }
        }
    };
}

handle!(
    /// A `function` declaration — free function, class method, interface
    /// default method, or free-impl method (see [`Function::owner`]).
    Function,
    FunctionLoc<'db>
);
handle!(
    /// A `class` declaration.
    Class,
    ClassLoc<'db>
);
handle!(
    /// An `enum` declaration.
    Enum,
    EnumLoc<'db>
);
handle!(
    /// An `interface` declaration.
    Interface,
    InterfaceLoc<'db>
);
handle!(
    /// A `type X = …` declaration.
    TypeAlias,
    TypeAliasLoc<'db>
);
handle!(
    /// A `client` declaration surviving as a dedicated item. Modern
    /// `client<llm>` syntax desugars to a [`Global`] with
    /// [`LetOrigin::Client`]; this handle covers whatever still allocates
    /// `Client` items.
    Client,
    ClientLoc<'db>
);
handle!(
    /// A `test` declaration.
    Test,
    TestLoc<'db>
);
handle!(
    /// A `template_string` declaration.
    TemplateString,
    TemplateStringLoc<'db>
);
handle!(
    /// A `retry_policy` declaration surviving as a dedicated item — modern
    /// syntax desugars to a [`Global`] with [`LetOrigin::RetryPolicy`].
    RetryPolicy,
    RetryPolicyLoc<'db>
);
handle!(
    /// A top-level `let` binding — including the desugared forms of
    /// `client<llm>` and `retry_policy` (see [`Global::origin`]).
    Global,
    LetLoc<'db>
);
handle!(
    /// An `implements I for T { … }` block (in-body blocks are normalized to
    /// the same representation). Impls have no name.
    Impl,
    ImplLoc<'db>
);

// ── Kind ─────────────────────────────────────────────────────────────────────

/// The kind tag for a [`Symbol`] — `DefinitionKind`'s namespace-level kinds
/// plus `Impl`, which has no name and therefore no `Definition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Class,
    Enum,
    Interface,
    TypeAlias,
    Function,
    TemplateString,
    Client,
    Test,
    RetryPolicy,
    Global,
    Impl,
}

impl SymbolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Enum => "enum",
            Self::Interface => "interface",
            Self::TypeAlias => "type",
            Self::Function => "function",
            Self::TemplateString => "template_string",
            Self::Client => "client",
            Self::Test => "test",
            Self::RetryPolicy => "retry_policy",
            Self::Global => "let",
            Self::Impl => "impl",
        }
    }
}

// ── Symbol: the sum ──────────────────────────────────────────────────────────

/// Any describable top-level item: the ten [`Definition`] kinds plus impls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Symbol<'db> {
    Class(Class<'db>),
    Enum(Enum<'db>),
    Interface(Interface<'db>),
    TypeAlias(TypeAlias<'db>),
    Function(Function<'db>),
    TemplateString(TemplateString<'db>),
    Client(Client<'db>),
    Test(Test<'db>),
    RetryPolicy(RetryPolicy<'db>),
    Global(Global<'db>),
    Impl(Impl<'db>),
}

impl<'db> From<Definition<'db>> for Symbol<'db> {
    fn from(def: Definition<'db>) -> Self {
        match def {
            Definition::Class(loc) => Self::Class(loc.into()),
            Definition::Enum(loc) => Self::Enum(loc.into()),
            Definition::Interface(loc) => Self::Interface(loc.into()),
            Definition::TypeAlias(loc) => Self::TypeAlias(loc.into()),
            Definition::Function(loc) => Self::Function(loc.into()),
            Definition::TemplateString(loc) => Self::TemplateString(loc.into()),
            Definition::Client(loc) => Self::Client(loc.into()),
            Definition::Test(loc) => Self::Test(loc.into()),
            Definition::RetryPolicy(loc) => Self::RetryPolicy(loc.into()),
            Definition::Let(loc) => Self::Global(loc.into()),
        }
    }
}

impl<'db> Symbol<'db> {
    /// The structural kind. A desugared `client<llm>`/`retry_policy` global
    /// reports `Global` here; [`Symbol::source_kind`] reports the kind as
    /// written in source.
    pub fn kind(self) -> SymbolKind {
        match self {
            Self::Class(_) => SymbolKind::Class,
            Self::Enum(_) => SymbolKind::Enum,
            Self::Interface(_) => SymbolKind::Interface,
            Self::TypeAlias(_) => SymbolKind::TypeAlias,
            Self::Function(_) => SymbolKind::Function,
            Self::TemplateString(_) => SymbolKind::TemplateString,
            Self::Client(_) => SymbolKind::Client,
            Self::Test(_) => SymbolKind::Test,
            Self::RetryPolicy(_) => SymbolKind::RetryPolicy,
            Self::Global(_) => SymbolKind::Global,
            Self::Impl(_) => SymbolKind::Impl,
        }
    }

    /// The declaration kind as written in BAML source: a [`Global`] that
    /// desugared from `client<llm>` or `retry_policy` reports that original
    /// kind.
    pub fn source_kind(self, db: &'db dyn Db) -> SymbolKind {
        match self {
            Self::Global(global) => match global.origin(db) {
                LetOrigin::Client => SymbolKind::Client,
                LetOrigin::RetryPolicy => SymbolKind::RetryPolicy,
                LetOrigin::Source => SymbolKind::Global,
            },
            other => other.kind(),
        }
    }

    /// The item's declared name; `None` for impls, which have none.
    pub fn name(self, db: &'db dyn Db) -> Option<Name> {
        match self {
            Self::Class(h) => Some(h.name(db)),
            Self::Enum(h) => Some(h.name(db)),
            Self::Interface(h) => Some(h.name(db)),
            Self::TypeAlias(h) => Some(h.name(db)),
            Self::Function(h) => Some(h.name(db)),
            Self::TemplateString(h) => Some(h.name(db)),
            Self::Client(h) => Some(h.name(db)),
            Self::Test(h) => Some(h.name(db)),
            Self::RetryPolicy(h) => Some(h.name(db)),
            Self::Global(h) => Some(h.name(db)),
            Self::Impl(_) => None,
        }
    }

    /// The leading `///` docstring, where the kind carries one.
    pub fn docstring(self, db: &'db dyn Db) -> Option<&'db str> {
        match self {
            Self::Class(h) => h.docstring(db),
            Self::Enum(h) => h.docstring(db),
            Self::Interface(h) => h.docstring(db),
            Self::TypeAlias(h) => h.docstring(db),
            Self::Function(h) => h.docstring(db),
            Self::Impl(h) => h.docstring(db),
            // These kinds carry no docstring in the item data today.
            Self::TemplateString(_)
            | Self::Client(_)
            | Self::Test(_)
            | Self::RetryPolicy(_)
            | Self::Global(_) => None,
        }
    }

    pub fn file(self, db: &'db dyn Db) -> SourceFile {
        match self {
            Self::Class(h) => h.file(db),
            Self::Enum(h) => h.file(db),
            Self::Interface(h) => h.file(db),
            Self::TypeAlias(h) => h.file(db),
            Self::Function(h) => h.file(db),
            Self::TemplateString(h) => h.file(db),
            Self::Client(h) => h.file(db),
            Self::Test(h) => h.file(db),
            Self::RetryPolicy(h) => h.file(db),
            Self::Global(h) => h.file(db),
            Self::Impl(h) => h.file(db),
        }
    }

    /// Full span of the declaration.
    pub fn span(self, db: &'db dyn Db) -> TextRange {
        match self {
            Self::Class(h) => h.span(db),
            Self::Enum(h) => h.span(db),
            Self::Interface(h) => h.span(db),
            Self::TypeAlias(h) => h.span(db),
            Self::Function(h) => h.span(db),
            Self::TemplateString(h) => h.span(db),
            Self::Client(h) => h.span(db),
            Self::Test(h) => h.span(db),
            Self::RetryPolicy(h) => h.span(db),
            Self::Global(h) => h.span(db),
            Self::Impl(h) => h.span(db),
        }
    }

    /// Whether this item was minted by the compiler rather than written by
    /// the user — `$stream` aliases/classes, `$new` client constructors, LLM
    /// function companions, and the like.
    ///
    /// Reliable, not heuristic: `$` cannot appear in a user identifier, and
    /// every compiler-synthesized top-level item is `$`-named. (Auto-derived
    /// *methods* like `to_json` are `$`-less — filter those by
    /// [`Function::origin`]; they never appear as namespace items.)
    pub fn is_synthetic(self, db: &'db dyn Db) -> bool {
        self.name(db)
            .is_some_and(|name| name.as_str().contains('$'))
    }

    /// Span of the name token; `None` for impls.
    pub fn name_span(self, db: &'db dyn Db) -> Option<TextRange> {
        match self {
            Self::Class(h) => Some(h.name_span(db)),
            Self::Enum(h) => Some(h.name_span(db)),
            Self::Interface(h) => Some(h.name_span(db)),
            Self::TypeAlias(h) => Some(h.name_span(db)),
            Self::Function(h) => Some(h.name_span(db)),
            Self::TemplateString(h) => Some(h.name_span(db)),
            Self::Client(h) => Some(h.name_span(db)),
            Self::Test(h) => Some(h.name_span(db)),
            Self::RetryPolicy(h) => Some(h.name_span(db)),
            Self::Global(h) => Some(h.name_span(db)),
            Self::Impl(_) => None,
        }
    }
}

// ── Per-kind syntactic methods ───────────────────────────────────────────────

impl<'db> Function<'db> {
    pub fn name(self, db: &'db dyn Db) -> Name {
        item_data::function_data(db, self.0).name.clone()
    }

    pub fn docstring(self, db: &'db dyn Db) -> Option<&'db str> {
        item_data::function_data(db, self.0).docstring.as_deref()
    }

    pub fn span(self, db: &'db dyn Db) -> TextRange {
        item_data::function_source_map(db, self.0).span
    }

    pub fn name_span(self, db: &'db dyn Db) -> TextRange {
        item_data::function_source_map(db, self.0).name_span
    }

    /// How this function came to exist (user-written, companion, derived, …).
    pub fn origin(self, db: &'db dyn Db) -> FunctionOrigin {
        item_data::function_data(db, self.0).metadata.origin
    }

    /// Whether the declaration is outside BAML's user-facing surface.
    pub fn is_language_internal(self, db: &'db dyn Db) -> bool {
        item_data::function_data(db, self.0)
            .metadata
            .is_language_internal
    }

    /// Declaration-site resolved signature: params, return, declared throws,
    /// own generic parameters. `Self` is bound for class methods; interface
    /// default and free-impl methods lower without a `Self` binding today
    /// (their `Self` mentions are `Ty::Error` — see the query's docs).
    pub fn signature(self, db: &'db dyn Db) -> &'db facts::FunctionSignatureTy {
        facts::function_signature(db, self.0)
    }

    /// The full throws surface: declared clause, effective contract, and the
    /// effective set partitioned into panics and ordinary errors.
    pub fn throws(self, db: &'db dyn Db) -> Throws<'db> {
        let effective = facts::effective_throws(db, self.0);
        let (panics, errors) = facts::throws_leaves(&effective)
            .into_iter()
            .partition(is_panic_type);
        Throws {
            declared: self.signature(db).declared_throws.as_ref(),
            effective,
            panics,
            errors,
        }
    }

    /// The function's own generic parameters with their resolved interface
    /// bounds, in declaration order.
    pub fn generic_params(self, db: &'db dyn Db) -> Vec<(ParamTy, Vec<InterfaceBound>)> {
        let bounds = facts::function_generic_bounds(db, self.0);
        self.signature(db)
            .generic_params
            .iter()
            .map(|param| {
                (
                    param.clone(),
                    bounds.get(param).cloned().unwrap_or_default(),
                )
            })
            .collect()
    }

    /// The item this function is a method of; `None` for a free function.
    pub fn owner(self, db: &'db dyn Db) -> Option<FunctionOwner<'db>> {
        item_data::method_owner(db, self.0).map(|owner| match owner {
            item_data::MethodOwner::Class(loc) => FunctionOwner::Class(loc.into()),
            item_data::MethodOwner::Interface(loc) => FunctionOwner::Interface(loc.into()),
            item_data::MethodOwner::FreeImpl(loc) => FunctionOwner::Impl(loc.into()),
        })
    }
}

/// The declaring container of a method — see [`Function::owner`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FunctionOwner<'db> {
    /// A class method, including methods of in-body/merged `implements`
    /// blocks.
    Class(Class<'db>),
    /// An interface default method.
    Interface(Interface<'db>),
    /// A method of a free `implements … for …` block.
    Impl(Impl<'db>),
}

/// A function's throws surface — see [`Function::throws`].
#[derive(Debug, Clone, PartialEq)]
pub struct Throws<'db> {
    /// The written `throws` clause, lowered; `None` when omitted.
    pub declared: Option<&'db Ty>,
    /// The effective contract: the declared clause when written, otherwise
    /// inferred from the body.
    pub effective: Ty,
    /// Leaves of the effective set that are panic types (classes in
    /// `baml.panics`).
    pub panics: Vec<Ty>,
    /// Every other leaf of the effective set — ordinary error classes, plus
    /// anything not classifiable as a panic (type variables, projections).
    pub errors: Vec<Ty>,
}

/// Whether a throws leaf is a panic: a class in the closed `baml.panics`
/// namespace. Everything else — including generic leaves like `E` or
/// `(Self as I).Error` — is treated as an ordinary error.
pub(crate) fn is_panic_type(ty: &Ty) -> bool {
    match ty {
        Ty::Class(qtn, ..) => {
            qtn.package().as_str() == "baml"
                && qtn.namespace().len() == 1
                && qtn.namespace()[0].as_str() == "panics"
        }
        _ => false,
    }
}

impl<'db> Class<'db> {
    pub fn name(self, db: &'db dyn Db) -> Name {
        item_data::class_data(db, self.0).name.clone()
    }

    /// The item's canonical qualified name: package, namespace path, name.
    pub fn qualified_name(self, db: &'db dyn Db) -> baml_type::QualifiedTypeName {
        let pkg = baml_compiler2_hir::file_package::file_package(db, self.0.file(db));
        baml_type::QualifiedTypeName::new(pkg.package, pkg.namespace_path, self.name(db))
    }

    pub fn docstring(self, db: &'db dyn Db) -> Option<&'db str> {
        item_data::class_data(db, self.0).docstring.as_deref()
    }

    pub fn span(self, db: &'db dyn Db) -> TextRange {
        item_data::class_source_map(db, self.0).span
    }

    pub fn name_span(self, db: &'db dyn Db) -> TextRange {
        item_data::class_source_map(db, self.0).name_span
    }

    /// The class's fields, with resolved types.
    pub fn fields(self, db: &'db dyn Db) -> Vec<Field<'db>> {
        (0..item_data::class_data(db, self.0).fields.len())
            .map(|index| Field {
                owner: FieldOwner::Class(self),
                index,
            })
            .collect()
    }

    /// The class's generic parameters with their resolved interface bounds,
    /// in declaration order. The bounds map is sparse — an unbounded
    /// parameter has no entry and gets an empty conjunction.
    pub fn generic_params(self, db: &'db dyn Db) -> Vec<(ParamTy, Vec<InterfaceBound>)> {
        let bounds = facts::class_generic_bounds(db, self.0);
        facts::class_generic_params(db, self.0)
            .into_iter()
            .map(|param| {
                let ifaces = bounds.get(&param).cloned().unwrap_or_default();
                (param, ifaces)
            })
            .collect()
    }

    /// Methods declared on the class — including in-body/merged `implements`
    /// block methods, which the item tree flattens into class membership.
    pub fn methods(self, db: &'db dyn Db) -> Vec<Function<'db>> {
        item_data::class_data(db, self.0)
            .methods
            .iter()
            .map(|&loc| loc.into())
            .collect()
    }

    /// The class's own `implements` blocks (in-body and same-file merged),
    /// in source order.
    pub fn impls(self, db: &'db dyn Db) -> Vec<Impl<'db>> {
        item_data::class_impls(db, self.0)
            .iter()
            .map(|&loc| loc.into())
            .collect()
    }
}

impl<'db> Enum<'db> {
    pub fn name(self, db: &'db dyn Db) -> Name {
        item_data::enum_data(db, self.0).name.clone()
    }

    /// The item's canonical qualified name: package, namespace path, name.
    pub fn qualified_name(self, db: &'db dyn Db) -> baml_type::QualifiedTypeName {
        let pkg = baml_compiler2_hir::file_package::file_package(db, self.0.file(db));
        baml_type::QualifiedTypeName::new(pkg.package, pkg.namespace_path, self.name(db))
    }

    /// The enum's variants, in declaration order.
    pub fn variants(self, db: &'db dyn Db) -> Vec<Variant<'db>> {
        (0..item_data::enum_data(db, self.0).variants.len())
            .map(|index| Variant { owner: self, index })
            .collect()
    }

    pub fn docstring(self, db: &'db dyn Db) -> Option<&'db str> {
        item_data::enum_data(db, self.0).docstring.as_deref()
    }

    pub fn span(self, db: &'db dyn Db) -> TextRange {
        item_data::enum_source_map(db, self.0).span
    }

    pub fn name_span(self, db: &'db dyn Db) -> TextRange {
        item_data::enum_source_map(db, self.0).name_span
    }
}

impl<'db> Interface<'db> {
    pub fn name(self, db: &'db dyn Db) -> Name {
        item_data::interface_data(db, self.0).name.clone()
    }

    /// The item's canonical qualified name: package, namespace path, name.
    pub fn qualified_name(self, db: &'db dyn Db) -> baml_type::QualifiedTypeName {
        let pkg = baml_compiler2_hir::file_package::file_package(db, self.0.file(db));
        baml_type::QualifiedTypeName::new(pkg.package, pkg.namespace_path, self.name(db))
    }

    pub fn docstring(self, db: &'db dyn Db) -> Option<&'db str> {
        item_data::interface_data(db, self.0).docstring.as_deref()
    }

    pub fn span(self, db: &'db dyn Db) -> TextRange {
        item_data::interface_source_map(db, self.0).span
    }

    pub fn name_span(self, db: &'db dyn Db) -> TextRange {
        item_data::interface_source_map(db, self.0).name_span
    }

    /// The interface's declared fields, with types resolved in the
    /// interface's own scope (symbolic `Self`).
    pub fn fields(self, db: &'db dyn Db) -> Vec<Field<'db>> {
        (0..item_data::interface_data(db, self.0).fields.len())
            .map(|index| Field {
                owner: FieldOwner::Interface(self),
                index,
            })
            .collect()
    }

    /// The interface's required methods (signatures without bodies), with
    /// `Self` left symbolic. Docstrings and name spans join by index from the
    /// PPIR data; the resolved signature comes from the type system.
    pub fn required_methods(self, db: &'db dyn Db) -> Vec<RequiredMethod<'db>> {
        (0..item_data::interface_data(db, self.0).required_methods.len())
            .map(|index| RequiredMethod { owner: self, index })
            .collect()
    }

    /// The interface's associated type declarations.
    pub fn assoc_types(self, db: &'db dyn Db) -> Vec<AssocType<'db>> {
        (0..item_data::interface_data(db, self.0).associated_types.len())
            .map(|index| AssocType { owner: self, index })
            .collect()
    }

    /// The interface's default methods (the ones with bodies).
    pub fn default_methods(self, db: &'db dyn Db) -> Vec<Function<'db>> {
        item_data::interface_data(db, self.0)
            .default_methods
            .iter()
            .map(|&loc| loc.into())
            .collect()
    }

    /// The parameters the interface *declares*, with their resolved bounds, in
    /// declaration order.
    ///
    /// Not the in-scope view: an interface's scope also carries the implicit
    /// `Self`, which is a parameter of every interface and so says nothing
    /// about this one. `interface Add<Rhs>` declares `Rhs`.
    pub fn generic_params(self, db: &'db dyn Db) -> Vec<(ParamTy, Vec<InterfaceBound>)> {
        let bounds = facts::interface_generic_bounds(db, self.0);
        facts::interface_generic_params(db, self.0)
            .into_iter()
            .map(|param| {
                let ifaces = bounds.get(&param).cloned().unwrap_or_default();
                (param, ifaces)
            })
            .collect()
    }
}

impl<'db> TypeAlias<'db> {
    pub fn name(self, db: &'db dyn Db) -> Name {
        item_data::type_alias_data(db, self.0).name.clone()
    }

    /// The item's canonical qualified name: package, namespace path, name.
    pub fn qualified_name(self, db: &'db dyn Db) -> baml_type::QualifiedTypeName {
        let pkg = baml_compiler2_hir::file_package::file_package(db, self.0.file(db));
        baml_type::QualifiedTypeName::new(pkg.package, pkg.namespace_path, self.name(db))
    }

    /// The aliased type, resolved.
    pub fn resolved(self, db: &'db dyn Db) -> Ty {
        facts::type_alias_resolved(db, self.0)
    }

    pub fn docstring(self, db: &'db dyn Db) -> Option<&'db str> {
        item_data::type_alias_data(db, self.0).docstring.as_deref()
    }

    pub fn span(self, db: &'db dyn Db) -> TextRange {
        item_data::type_alias_source_map(db, self.0).span
    }

    pub fn name_span(self, db: &'db dyn Db) -> TextRange {
        item_data::type_alias_source_map(db, self.0).name_span
    }
}

impl<'db> Client<'db> {
    pub fn name(self, db: &'db dyn Db) -> Name {
        item_data::client_data(db, self.0).name.clone()
    }

    pub fn span(self, db: &'db dyn Db) -> TextRange {
        item_data::client_source_map(db, self.0).span
    }

    pub fn name_span(self, db: &'db dyn Db) -> TextRange {
        item_data::client_source_map(db, self.0).name_span
    }
}

impl<'db> Test<'db> {
    pub fn name(self, db: &'db dyn Db) -> Name {
        item_data::test_data(db, self.0).name.clone()
    }

    /// The functions this test exercises, as declared.
    pub fn function_refs(self, db: &'db dyn Db) -> &'db [Name] {
        &item_data::test_data(db, self.0).function_refs
    }

    pub fn span(self, db: &'db dyn Db) -> TextRange {
        item_data::test_source_map(db, self.0).span
    }

    pub fn name_span(self, db: &'db dyn Db) -> TextRange {
        item_data::test_source_map(db, self.0).name_span
    }
}

impl<'db> TemplateString<'db> {
    pub fn name(self, db: &'db dyn Db) -> Name {
        item_data::template_string_data(db, self.0).name.clone()
    }

    pub fn span(self, db: &'db dyn Db) -> TextRange {
        item_data::template_string_source_map(db, self.0).span
    }

    pub fn name_span(self, db: &'db dyn Db) -> TextRange {
        item_data::template_string_source_map(db, self.0).name_span
    }
}

impl<'db> RetryPolicy<'db> {
    pub fn name(self, db: &'db dyn Db) -> Name {
        item_data::retry_policy_data(db, self.0).name.clone()
    }

    pub fn span(self, db: &'db dyn Db) -> TextRange {
        item_data::retry_policy_source_map(db, self.0).span
    }

    pub fn name_span(self, db: &'db dyn Db) -> TextRange {
        item_data::retry_policy_source_map(db, self.0).name_span
    }
}

impl<'db> Global<'db> {
    pub fn name(self, db: &'db dyn Db) -> Name {
        item_data::let_data(db, self.0).name.clone()
    }

    /// What this binding desugared from (`client<llm>`, `retry_policy`, or a
    /// source-written `let`).
    pub fn origin(self, db: &'db dyn Db) -> LetOrigin {
        item_data::let_data(db, self.0).origin
    }

    pub fn span(self, db: &'db dyn Db) -> TextRange {
        item_data::let_source_map(db, self.0).span
    }

    pub fn name_span(self, db: &'db dyn Db) -> TextRange {
        item_data::let_source_map(db, self.0).name_span
    }
}

impl<'db> Impl<'db> {
    /// Leading `///` docstring — populated for free `implements … for …`
    /// blocks; in-body blocks carry none today.
    pub fn docstring(self, db: &'db dyn Db) -> Option<&'db str> {
        item_data::impl_block_data(db, self.0).docstring.as_deref()
    }

    pub fn span(self, db: &'db dyn Db) -> TextRange {
        item_data::impl_block_source_map(db, self.0).span
    }

    /// The block's method overrides, in source order.
    pub fn methods(self, db: &'db dyn Db) -> Vec<Function<'db>> {
        item_data::impl_block_data(db, self.0)
            .methods
            .iter()
            .map(|&loc| loc.into())
            .collect()
    }

    /// The implemented interface; `None` when the block is malformed.
    pub fn interface(self, db: &'db dyn Db) -> Option<Interface<'db>> {
        facts::impl_data(db, self.0).map(|data| data.interface.into())
    }

    /// The interface's generic arguments as written on this block.
    pub fn interface_args(self, db: &'db dyn Db) -> Option<&'db [Ty]> {
        facts::impl_data(db, self.0).map(|data| data.interface_args.as_slice())
    }

    /// The resolved implementor pattern — may carry the block's own rigid
    /// type variables (`T[]` for `implements<T extends Comparable> Sortable
    /// for T[]`).
    pub fn for_ty(self, db: &'db dyn Db) -> Option<&'db Ty> {
        facts::impl_data(db, self.0).map(|data| &data.for_ty_pattern)
    }

    /// The block's generic parameters with their resolved interface bounds.
    pub fn generic_params(self, db: &'db dyn Db) -> Option<&'db [(ParamTy, Vec<InterfaceBound>)]> {
        facts::impl_data(db, self.0).map(|data| data.generic_params.as_slice())
    }

    /// The block's associated-type bindings, resolved
    /// (`type SortError = T.CompareError` → `(SortError, (T as
    /// baml.Comparable).CompareError)`).
    pub fn assoc_bindings(self, db: &'db dyn Db) -> Option<&'db [(Name, Ty)]> {
        facts::impl_data(db, self.0).map(|data| data.associated_types.as_slice())
    }

    /// Every method this impl supplies, rustdoc-style: the block's own
    /// overrides plus the interface's default methods it did not override.
    /// Empty for a malformed block.
    pub fn all_methods(self, db: &'db dyn Db) -> Vec<ImplMethod<'db>> {
        let Some(data) = facts::impl_data(db, self.0) else {
            return Vec::new();
        };
        let mut out: Vec<ImplMethod<'db>> = data
            .methods
            .iter()
            .map(|&loc| ImplMethod {
                function: loc.into(),
                from_default: false,
            })
            .collect();
        let overridden: Vec<Name> = out.iter().map(|m| m.function.name(db)).collect();
        for &default_loc in &item_data::interface_data(db, data.interface).default_methods {
            let default: Function<'db> = default_loc.into();
            if !overridden.contains(&default.name(db)) {
                out.push(ImplMethod {
                    function: default,
                    from_default: true,
                });
            }
        }
        out
    }
}

/// One method supplied by an impl — an override, or an interface default the
/// impl inherited. See [`Impl::all_methods`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImplMethod<'db> {
    pub function: Function<'db>,
    /// `true` when the method is the interface's default rather than an
    /// override written in the block.
    pub from_default: bool,
}

// ── Package / Namespace ──────────────────────────────────────────────────────

/// A package — `user` for the project's own code, or a builtin package
/// (`baml`, `assert`, `log`, `testing`, `reflect`, `boundary`).
// Manual `Debug`: salsa interned ids have an opaque repr and no derive.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Package<'db>(pub(crate) PackageId<'db>);

impl std::fmt::Debug for Package<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Package").finish_non_exhaustive()
    }
}

impl<'db> Package<'db> {
    /// The package with this exact name. Always constructible — a package
    /// with no items simply lists nothing.
    pub fn named(db: &'db dyn Db, name: &str) -> Self {
        Self(PackageId::new(db, Name::new(name)))
    }

    /// The project's own package.
    pub fn user(db: &'db dyn Db) -> Self {
        Self::named(db, "user")
    }

    pub fn name(self, db: &'db dyn Db) -> Name {
        self.0.name(db)
    }

    /// The package with this name, when such a package can exist: `user`,
    /// or one of the builtin stdlib packages. `None` otherwise — a routing
    /// guard for string resolution.
    pub fn named_checked(db: &'db dyn Db, name: &str) -> Option<Self> {
        (name == "user" || baml_builtins2::stdlib_package_names().contains(&name))
            .then(|| Self::named(db, name))
    }

    /// The namespace at exactly this path, if the package has it.
    pub fn namespace(self, db: &'db dyn Db, path: &[String]) -> Option<Namespace<'db>> {
        let path: Vec<Name> = path.iter().map(Name::new).collect();
        baml_compiler2_ppir::package_items(db, self.0)
            .namespaces
            .contains_key(&path)
            .then(|| Namespace(NamespaceId::new(db, self.0.name(db), path)))
    }

    /// The package's namespaces, root first, then sorted by path.
    pub fn namespaces(self, db: &'db dyn Db) -> Vec<Namespace<'db>> {
        let items = baml_compiler2_ppir::package_items(db, self.0);
        let mut paths: Vec<&Vec<Name>> = items.namespaces.keys().collect();
        paths.sort();
        paths
            .into_iter()
            .map(|path| Namespace(NamespaceId::new(db, self.0.name(db), path.clone())))
            .collect()
    }
}

/// One namespace within a package.
// Manual `Debug`: salsa interned ids have an opaque repr and no derive.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Namespace<'db>(pub(crate) NamespaceId<'db>);

impl std::fmt::Debug for Namespace<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Namespace").finish_non_exhaustive()
    }
}

impl<'db> Namespace<'db> {
    pub fn package(self, db: &'db dyn Db) -> Package<'db> {
        Package(PackageId::new(db, self.0.package(db)))
    }

    /// The dotted namespace path; empty for the package root.
    pub fn path(self, db: &'db dyn Db) -> Vec<Name> {
        self.0.path(db)
    }

    /// The type-space item with this name, if any.
    pub fn type_named(self, db: &'db dyn Db, name: &str) -> Option<Symbol<'db>> {
        baml_compiler2_ppir::namespace_items(db, self.0)
            .types
            .get(&Name::new(name))
            .map(|def| Symbol::from(*def))
    }

    /// The value-space item with this name, if any.
    pub fn value_named(self, db: &'db dyn Db, name: &str) -> Option<Symbol<'db>> {
        baml_compiler2_ppir::namespace_items(db, self.0)
            .values
            .get(&Name::new(name))
            .map(|def| Symbol::from(*def))
    }

    /// Every named item this namespace contributes, types then values, each
    /// sorted by name. (Impls are unnamed and therefore not listed here;
    /// reach them through [`Class::impls`] or file enumeration.)
    pub fn items(self, db: &'db dyn Db) -> Vec<(Name, Symbol<'db>)> {
        let ns_items = baml_compiler2_ppir::namespace_items(db, self.0);
        let mut types: Vec<(Name, Symbol<'db>)> = ns_items
            .types
            .iter()
            .map(|(name, def)| (name.clone(), Symbol::from(*def)))
            .collect();
        types.sort_by(|(a, _), (b, _)| a.cmp(b));
        let mut values: Vec<(Name, Symbol<'db>)> = ns_items
            .values
            .iter()
            .map(|(name, def)| (name.clone(), Symbol::from(*def)))
            .collect();
        values.sort_by(|(a, _), (b, _)| a.cmp(b));
        types.extend(values);
        types
    }
}

// ── Member composites ────────────────────────────────────────────────────────

/// A field of a class or interface, addressed by declaration index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Field<'db> {
    pub(crate) owner: FieldOwner<'db>,
    pub(crate) index: usize,
}

/// The declaring container of a [`Field`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldOwner<'db> {
    Class(Class<'db>),
    Interface(Interface<'db>),
}

impl<'db> Field<'db> {
    pub fn owner(self) -> FieldOwner<'db> {
        self.owner
    }

    pub fn name(self, db: &'db dyn Db) -> Name {
        match self.owner {
            FieldOwner::Class(class) => item_data::class_data(db, class.0).fields[self.index]
                .name
                .clone(),
            FieldOwner::Interface(iface) => item_data::interface_data(db, iface.0).fields
                [self.index]
                .name
                .clone(),
        }
    }

    /// The field's resolved type. Interface field types resolve with `Self`
    /// symbolic.
    pub fn ty(self, db: &'db dyn Db) -> &'db Ty {
        match self.owner {
            FieldOwner::Class(class) => &facts::class_fields(db, class.0)[self.index].1,
            FieldOwner::Interface(iface) => {
                &facts::interface_fields(db, iface.0).fields[self.index].1
            }
        }
    }

    pub fn docstring(self, db: &'db dyn Db) -> Option<&'db str> {
        match self.owner {
            FieldOwner::Class(class) => item_data::class_data(db, class.0).fields[self.index]
                .docstring
                .as_deref(),
            FieldOwner::Interface(iface) => item_data::interface_data(db, iface.0).fields
                [self.index]
                .docstring
                .as_deref(),
        }
    }

    pub fn name_span(self, db: &'db dyn Db) -> TextRange {
        match self.owner {
            FieldOwner::Class(class) => {
                item_data::class_source_map(db, class.0).field_name_spans[self.index]
            }
            FieldOwner::Interface(iface) => {
                item_data::interface_source_map(db, iface.0).field_name_spans[self.index]
            }
        }
    }
}

/// A variant of an enum, addressed by declaration index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Variant<'db> {
    pub(crate) owner: Enum<'db>,
    pub(crate) index: usize,
}

impl<'db> Variant<'db> {
    pub fn owner(self) -> Enum<'db> {
        self.owner
    }

    pub fn name(self, db: &'db dyn Db) -> Name {
        item_data::enum_data(db, self.owner.0).variants[self.index]
            .name
            .clone()
    }

    pub fn docstring(self, db: &'db dyn Db) -> Option<&'db str> {
        item_data::enum_data(db, self.owner.0).variants[self.index]
            .docstring
            .as_deref()
    }

    pub fn name_span(self, db: &'db dyn Db) -> TextRange {
        item_data::enum_source_map(db, self.owner.0).variant_name_spans[self.index]
    }
}

/// A required (bodyless) method of an interface, addressed by declaration
/// index. Default methods are ordinary [`Function`]s instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequiredMethod<'db> {
    pub(crate) owner: Interface<'db>,
    pub(crate) index: usize,
}

impl<'db> RequiredMethod<'db> {
    pub fn owner(self) -> Interface<'db> {
        self.owner
    }

    pub fn name(self, db: &'db dyn Db) -> Name {
        item_data::interface_data(db, self.owner.0).required_methods[self.index]
            .name
            .clone()
    }

    pub fn docstring(self, db: &'db dyn Db) -> Option<&'db str> {
        item_data::interface_data(db, self.owner.0).required_methods[self.index]
            .docstring
            .as_deref()
    }

    pub fn name_span(self, db: &'db dyn Db) -> TextRange {
        item_data::interface_source_map(db, self.owner.0).required_method_spans[self.index]
            .name_span
    }

    /// The declaration-site resolved signature, `Self` left symbolic.
    pub fn resolved(self, db: &'db dyn Db) -> &'db facts::ResolvedInterfaceMethod {
        &facts::interface_required_methods(db, self.owner.0)[self.index]
    }
}

/// An associated type declared on an interface, addressed by declaration
/// index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssocType<'db> {
    pub(crate) owner: Interface<'db>,
    pub(crate) index: usize,
}

impl<'db> AssocType<'db> {
    pub fn owner(self) -> Interface<'db> {
        self.owner
    }

    pub fn name(self, db: &'db dyn Db) -> Name {
        item_data::interface_data(db, self.owner.0).associated_types[self.index]
            .name
            .clone()
    }

    /// The declared default, lowered against the interface's own scope
    /// (symbolic `Self`); `None` when the declaration has no default.
    pub fn default_ty(self, db: &'db dyn Db) -> Option<Ty> {
        facts::interface_associated_type_default(db, self.owner.0, self.name(db))
    }

    pub fn name_span(self, db: &'db dyn Db) -> TextRange {
        item_data::interface_source_map(db, self.owner.0).associated_type_spans[self.index]
            .name_span
    }
}

// ── Impl attachment (rustdoc-style, lossy head matching) ────────────────────

/// Every impl block in the project — user package and builtins alike, in
/// file order. Impl attachment must see the whole project because the orphan
/// rule allows an impl to live downstream of the type it implements
/// (`implements MyIface for int` in user code attaches to `baml.Int`).
pub(crate) fn project_impls(db: &dyn Db) -> Vec<Impl<'_>> {
    baml_compiler2_hir::compiler2_all_files(db)
        .into_iter()
        .flat_map(|file| item_data::file_impls(db, file).iter().copied())
        .map(Impl::from)
        .collect()
}

/// Impls whose `for`-head matches `decl_head` (or blanket impls), grouped
/// stably: sorted by implemented interface name, then file order.
fn impls_attaching_to<'db>(db: &'db dyn Db, decl_head: &crate::head::TyHead) -> Vec<Impl<'db>> {
    let mut matching: Vec<(String, Impl<'db>)> = project_impls(db)
        .into_iter()
        .filter_map(|imp| {
            let data = facts::impl_data(db, imp.0)?;
            let impl_head = crate::head::ty_head(&data.for_ty_pattern)?;
            if crate::head::impl_attaches(&impl_head, decl_head) {
                let iface_name = imp
                    .interface(db)
                    .map(|iface| iface.qualified_name(db).render_dotted(false))
                    .unwrap_or_default();
                Some((iface_name, imp))
            } else {
                None
            }
        })
        .collect();
    matching.sort_by(|(a, _), (b, _)| a.cmp(b));
    matching.into_iter().map(|(_, imp)| imp).collect()
}

impl<'db> Class<'db> {
    /// Every impl in the project attaching to this class by head — its own
    /// (in-body and merged) blocks plus free impls anywhere, including
    /// generic ones (`implements<T extends Comparable> Sortable for T[]`
    /// attaches to `baml.Array`). Bounds are attached lossily: they are
    /// listed, not proven.
    pub fn trait_impls(self, db: &'db dyn Db) -> Vec<Impl<'db>> {
        impls_attaching_to(db, &crate::head::TyHead::Nominal(self.qualified_name(db)))
    }
}

impl<'db> Enum<'db> {
    /// Every impl in the project attaching to this enum by head.
    pub fn trait_impls(self, db: &'db dyn Db) -> Vec<Impl<'db>> {
        impls_attaching_to(db, &crate::head::TyHead::Nominal(self.qualified_name(db)))
    }
}

impl<'db> Interface<'db> {
    /// Every impl in the project implementing this interface, in file order.
    pub fn implementors(self, db: &'db dyn Db) -> Vec<Impl<'db>> {
        project_impls(db)
            .into_iter()
            .filter(|imp| facts::impl_data(db, imp.0).is_some_and(|data| data.interface == self.0))
            .collect()
    }
}

// ── Members: named lookup + the member sum ──────────────────────────────────

/// Any named member of a type: a method (class method, interface default, or
/// impl override — all [`Function`]s), an interface required method, a field,
/// an enum variant, or an associated type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Member<'db> {
    Method(Function<'db>),
    RequiredMethod(RequiredMethod<'db>),
    Field(Field<'db>),
    Variant(Variant<'db>),
    AssocType(AssocType<'db>),
}

impl<'db> Member<'db> {
    pub fn name(self, db: &'db dyn Db) -> Name {
        match self {
            Self::Method(m) => m.name(db),
            Self::RequiredMethod(m) => m.name(db),
            Self::Field(f) => f.name(db),
            Self::Variant(v) => v.name(db),
            Self::AssocType(a) => a.name(db),
        }
    }

    pub fn name_span(self, db: &'db dyn Db) -> TextRange {
        match self {
            Self::Method(m) => m.name_span(db),
            Self::RequiredMethod(m) => m.name_span(db),
            Self::Field(f) => f.name_span(db),
            Self::Variant(v) => v.name_span(db),
            Self::AssocType(a) => a.name_span(db),
        }
    }
}

impl<'db> Class<'db> {
    /// Look up a member by name: methods first, then fields.
    pub fn member_named(self, db: &'db dyn Db, name: &str) -> Option<Member<'db>> {
        if let Some(method) = self
            .methods(db)
            .into_iter()
            .find(|m| m.name(db).as_str() == name)
        {
            return Some(Member::Method(method));
        }
        self.fields(db)
            .into_iter()
            .find(|f| f.name(db).as_str() == name)
            .map(Member::Field)
    }
}

impl<'db> Enum<'db> {
    /// Look up a variant by name.
    pub fn member_named(self, db: &'db dyn Db, name: &str) -> Option<Member<'db>> {
        self.variants(db)
            .into_iter()
            .find(|v| v.name(db).as_str() == name)
            .map(Member::Variant)
    }
}

impl<'db> Interface<'db> {
    /// Look up a member by name: default methods, then required methods,
    /// then fields, then associated types.
    pub fn member_named(self, db: &'db dyn Db, name: &str) -> Option<Member<'db>> {
        if let Some(method) = self
            .default_methods(db)
            .into_iter()
            .find(|m| m.name(db).as_str() == name)
        {
            return Some(Member::Method(method));
        }
        if let Some(required) = self
            .required_methods(db)
            .into_iter()
            .find(|m| m.name(db).as_str() == name)
        {
            return Some(Member::RequiredMethod(required));
        }
        if let Some(field) = self
            .fields(db)
            .into_iter()
            .find(|f| f.name(db).as_str() == name)
        {
            return Some(Member::Field(field));
        }
        self.assoc_types(db)
            .into_iter()
            .find(|a| a.name(db).as_str() == name)
            .map(Member::AssocType)
    }
}

impl<'db> Symbol<'db> {
    /// Look up a member of this symbol by name, where the kind has members.
    pub fn member_named(self, db: &'db dyn Db, name: &str) -> Option<Member<'db>> {
        match self {
            Self::Class(c) => c.member_named(db, name),
            Self::Enum(e) => e.member_named(db, name),
            Self::Interface(i) => i.member_named(db, name),
            Self::TypeAlias(_)
            | Self::Function(_)
            | Self::TemplateString(_)
            | Self::Client(_)
            | Self::Test(_)
            | Self::RetryPolicy(_)
            | Self::Global(_)
            | Self::Impl(_) => None,
        }
    }
}
