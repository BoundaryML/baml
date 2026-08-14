//! The export IR: one serializable document describing a package's surface.
//!
//! A direct projection of the handle layer — every field corresponds to a
//! handle method, and the whole document is deterministic for a given source
//! state, so committed artifacts diff meaningfully. Ordering is by list kind:
//! items, impls, methods, and id lists are name- or id-sorted, while member
//! lists (fields, variants, associated types, required methods) keep
//! declaration order, which is the meaningful order for a reader.
//!
//! Types are exported as [`TyRef`] — a canonical display string plus a
//! resolved head reference — rather than as structural trees. That is the
//! API-review hybrid (rendered text you can read and diff, references you
//! can link), and it sidesteps tree serialization of recursive types
//! entirely. `unresolved` is computed structurally: a type that fails the
//! `RuntimeTy` narrowing carries a compiler sentinel (today: free-impl
//! signatures whose `Self` has no binding) and is flagged rather than
//! silently exported as if real.
//!
//! Impls are top-level records referenced by id from the items they attach
//! to — a blanket impl (`implements<T> Concrete for T`) attaches to every
//! item and must not be duplicated into each. The export set is explicit:
//! synthetic items (`$stream` companions, `$new` constructors) are listed
//! and flagged, never silently dropped.
//!
//! One document covers one package. References may cross packages — a field
//! type's head, an attached impl declared downstream — and stay
//! *interpretable* without any lookup because ids are self-describing
//! (`X:user.impl[user.Renderer for int]` carries kind, package, and path).
//! The referent's full record lives in its own package's export; there is
//! deliberately no rustdoc-style stub table, which exists there only because
//! opaque integer ids need one. Note the asymmetry: an item's `impls` list
//! is an *attachment view* (project-wide), while the top-level `impls` array
//! is a *declaration set* (this package only).

use std::fmt::Write as _;

use baml_type::{Interface as InterfaceBound, ParamTy, RuntimeTy, Ty};
use serde::Serialize;

use crate::{
    Db,
    display::TyDisplayFormat,
    handles::{Field, Function, Impl, Package, RequiredMethod, Symbol, Variant},
    head::{TyHead, ty_head},
    ids::SymbolId,
};

/// Bumped on every breaking change to this schema. Consumers should check it
/// before reading anything else.
pub const FORMAT_VERSION: u32 = 1;

// ── Leaf shapes ──────────────────────────────────────────────────────────────

/// A type in the export: canonical rendering + resolved head reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TyRef {
    /// Canonical, context-free rendering (`baml.errors.Io`, `T[]`).
    pub display: String,
    /// The `SymbolId` of the type's nominal head, when it has one —
    /// the link target for cross-referencing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    /// `true` when the type carries a compiler sentinel — an `!error` or
    /// `unknown` leaf, e.g. the unbound `Self` in today's free-impl
    /// signatures. Consumers must not treat the display string as a real
    /// type. Deliberately-symbolic forms (`(Self as I).Member`, free type
    /// variables) are *not* flagged: they are the correct declaration-site
    /// types.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unresolved: bool,
}

impl TyRef {
    pub fn of(ty: &Ty) -> Self {
        let head = match ty_head(ty) {
            Some(TyHead::Nominal(qtn)) => Some(
                SymbolId {
                    kind: crate::ids::IdKind::Type,
                    owner: crate::ids::Owner::Path {
                        package: qtn.package().to_string(),
                        namespace: qtn.namespace().iter().map(ToString::to_string).collect(),
                        name: qtn.name().to_string(),
                    },
                    member: None,
                }
                .to_string(),
            ),
            Some(TyHead::Function | TyHead::Future | TyHead::Blanket) | None => None,
        };
        Self {
            display: TyDisplayFormat::Canonical.render(ty),
            head,
            // `RuntimeTy` excludes exactly the compiler-sentinel axis
            // (`Error`/`Unknown`/`Evolving*`/`Infer`) while keeping symbolic
            // projections and type variables — the precise "is this a real
            // type" oracle.
            unresolved: RuntimeTy::try_from(ty).is_err(),
        }
    }
}

/// A generic parameter with its bounds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GenericExport {
    pub name: String,
    /// Bound interfaces, rendered (`baml.Comparable`); listed, not proven.
    pub bounds: Vec<String>,
}

fn generic_export(param: &ParamTy, bounds: &[InterfaceBound]) -> GenericExport {
    GenericExport {
        name: param.as_str().to_string(),
        bounds: bounds
            .iter()
            .map(|b| {
                let mut s = b.name.render_dotted(false);
                if !b.generics.is_empty() {
                    let args: Vec<String> = b
                        .generics
                        .iter()
                        .map(|ty| TyDisplayFormat::Canonical.render(ty))
                        .collect();
                    let _ = write!(s, "<{}>", args.join(", "));
                }
                s
            })
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParamExport {
    pub name: String,
    pub ty: TyRef,
    /// A default-valued parameter (named-only at call sites).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SignatureExport {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub generics: Vec<GenericExport>,
    pub params: Vec<ParamExport>,
    pub returns: TyRef,
    /// The effective contract — declared when written, inferred otherwise.
    /// Only the effective set is exported: a consumer cares what a call can
    /// raise, not whether the author wrote it down. The panics/errors split
    /// stays on the handle layer, where `Throws` still carries it.
    pub throws: TyRef,
}

/// Where a declaration lives: file plus byte span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceExport {
    pub file: String,
    pub start: u32,
    pub end: u32,
}

// ── Member shapes ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FunctionExport {
    /// Where this record lives. A method reached through an impl block is
    /// addressed *through* it — `M:(int as baml.ops.Add<bigint>).add` — because
    /// the same declaration can be re-listed by many blocks and each listing
    /// needs its own address.
    ///
    /// One id may appear on two records when they are the same symbol seen two
    /// ways: a method written in a class body's `implements` block is listed
    /// both as a method of the class and as a method of the block. Two
    /// *different* symbols never share one.
    pub id: String,
    /// Where the code is written, when that is somewhere else — an inherited
    /// default names the interface method it came from. Absent when [`id`]
    /// already names the declaration, which is the case for every method a
    /// block writes itself.
    ///
    /// [`id`]: Self::id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared_by: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docstring: Option<String>,
    /// `true` for compiler-minted companions (`$`-named) and derives.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub synthetic: bool,
    /// `true` when this entry is an interface default the impl inherited
    /// rather than an override (only set under [`ImplExport::methods`]).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub from_default: bool,
    pub signature: SignatureExport,
    pub source: SourceExport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldExport {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docstring: Option<String>,
    pub ty: TyRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VariantExport {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssocTypeExport {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<TyRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RequiredMethodExport {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docstring: Option<String>,
    pub signature: SignatureExport,
}

/// One impl block, top-level, referenced by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImplExport {
    /// Structural id: `X:<pkg>.impl[<interface> for <for-type>]`, with a
    /// `#n` suffix disambiguating same-headed blocks in declaration order.
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docstring: Option<String>,
    pub interface: String,
    /// The implemented interface's `SymbolId`.
    pub interface_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub interface_args: Vec<TyRef>,
    pub for_ty: TyRef,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub generics: Vec<GenericExport>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub assoc_bindings: Vec<AssocBindingExport>,
    /// Overrides plus inherited defaults, sorted by name.
    pub methods: Vec<FunctionExport>,
    pub source: SourceExport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssocBindingExport {
    pub name: String,
    pub ty: TyRef,
}

// ── Item records ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemExport {
    pub id: String,
    pub kind: crate::SymbolKind,
    pub name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub namespace: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docstring: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub synthetic: bool,
    pub source: SourceExport,
    #[serde(flatten)]
    pub detail: ItemDetail,
}

/// Kind-specific payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "detail", rename_all = "snake_case")]
pub enum ItemDetail {
    Class {
        #[serde(skip_serializing_if = "Vec::is_empty")]
        generics: Vec<GenericExport>,
        fields: Vec<FieldExport>,
        /// Inherent + in-body-impl methods (the item tree flattens both).
        methods: Vec<FunctionExport>,
        /// Ids of every impl attaching to this class by head, project-wide.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        impls: Vec<String>,
    },
    Enum {
        variants: Vec<VariantExport>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        impls: Vec<String>,
    },
    Interface {
        #[serde(skip_serializing_if = "Vec::is_empty")]
        generics: Vec<GenericExport>,
        /// Fields an implementor must provide, resolved in the interface's own
        /// scope (symbolic `Self`). Empty for the common method-only interface.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        fields: Vec<FieldExport>,
        assoc_types: Vec<AssocTypeExport>,
        required_methods: Vec<RequiredMethodExport>,
        default_methods: Vec<FunctionExport>,
        /// Ids of every impl implementing this interface, project-wide.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        implementors: Vec<String>,
    },
    TypeAlias {
        resolved: TyRef,
    },
    Function {
        signature: SignatureExport,
    },
    /// Kinds with no typed payload today (template strings, clients, tests,
    /// retry policies, globals).
    Plain {},
}

/// The whole-package document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackageExport {
    pub format_version: u32,
    pub package: String,
    /// Every named item, synthetic included, sorted by id.
    pub items: Vec<ItemExport>,
    /// Every impl block declared in this package, sorted by id.
    pub impls: Vec<ImplExport>,
}

// ── Projection ───────────────────────────────────────────────────────────────

/// Export one package's full surface.
pub fn export_package<'db>(db: &'db dyn Db, package: Package<'db>) -> PackageExport {
    let impl_index = ImplIndex::build(db);

    let mut items = Vec::new();
    for namespace in package.namespaces(db) {
        for (_, symbol) in namespace.items(db) {
            if let Some(item) = export_item(db, symbol, &impl_index) {
                items.push(item);
            }
        }
    }
    items.sort_by(|a, b| a.id.cmp(&b.id));

    let mut impls: Vec<ImplExport> = impl_index
        .exports
        .into_iter()
        .filter(|(imp, _)| {
            baml_compiler2_hir::file_package::file_package(db, imp.file(db)).package
                == package.name(db)
        })
        .map(|(_, export)| export)
        .collect();
    impls.sort_by(|a, b| a.id.cmp(&b.id));

    PackageExport {
        format_version: FORMAT_VERSION,
        package: package.name(db).to_string(),
        items,
        impls,
    }
}

/// Pre-built ids and exports for every resolvable impl in the project, so
/// item records can reference impls in any package.
struct ImplIndex<'db> {
    exports: Vec<(Impl<'db>, ImplExport)>,
}

impl<'db> ImplIndex<'db> {
    fn build(db: &'db dyn Db) -> Self {
        let mut exports = Vec::new();
        for imp in crate::handles::project_impls(db) {
            let Some(export) = export_impl(db, imp) else {
                continue;
            };
            exports.push((imp, export));
        }
        Self { exports }
    }

    fn ids_for_class_head(&self, db: &dyn Db, head: &TyHead) -> Vec<String> {
        let mut ids: Vec<String> = self
            .exports
            .iter()
            .filter(|(imp, _)| {
                crate::facts::impl_data(db, imp.loc())
                    .and_then(|data| ty_head(&data.for_ty_pattern))
                    .is_some_and(|impl_head| crate::head::impl_attaches(&impl_head, head))
            })
            .map(|(_, export)| export.id.clone())
            .collect();
        ids.sort();
        ids
    }

    fn ids_for_interface(&self, db: &dyn Db, iface: crate::Interface<'_>) -> Vec<String> {
        let mut ids: Vec<String> = self
            .exports
            .iter()
            .filter(|(imp, _)| {
                crate::facts::impl_data(db, imp.loc())
                    .is_some_and(|data| data.interface == iface.loc())
            })
            .map(|(_, export)| export.id.clone())
            .collect();
        ids.sort();
        ids
    }
}

fn source_export(
    db: &dyn Db,
    file: baml_base::SourceFile,
    span: text_size::TextRange,
) -> SourceExport {
    SourceExport {
        file: file.path(db).to_string_lossy().into_owned(),
        start: span.start().into(),
        end: span.end().into(),
    }
}

/// One function record. `block` is the id of the impl block this entry is
/// listed under, when it is listed under one.
///
/// An impl entry is addressed under its block rather than by the declaring
/// symbol's id: an inherited default is re-listed by every implementor (13
/// impls inherit `baml.iter.Iterator.chain`), and a method declared in a free
/// impl has no symbol id at all, so the declaration's id is not a key. The
/// declaring id is kept in `declared_by`, which is what a consumer dedupes on
/// when it wants to treat one declaration as one thing.
fn function_export(
    db: &dyn Db,
    function: Function<'_>,
    from_default: bool,
    via: Option<Impl<'_>>,
) -> FunctionExport {
    let sig = function.signature(db);
    let name = function.name(db);
    let declared = SymbolId::of_symbol(db, Symbol::Function(function)).map(|id| id.to_string());
    let (id, declared_by) = match via {
        // Reached through an impl block, so addressed through it, in the
        // qualified path a caller would actually write. This used to be
        // `<block id>::<name>`, which addressed the record perfectly well and
        // was spelled in a language BAML is not: `::` appears nowhere in the
        // grammar, and an id exists to be pasted back into a tool.
        Some(imp) => {
            let qualified = SymbolId::impl_owner(db, imp)
                .map(|owner| {
                    SymbolId {
                        kind: crate::ids::IdKind::Method,
                        owner,
                        member: Some(name.to_string()),
                    }
                    .to_string()
                })
                .unwrap_or_default();
            // `declared_by` is only worth stating when it differs — an
            // inherited default lives on the interface, while a method the
            // block writes itself is already named by `id`.
            let declared_by = declared.filter(|declared| declared != &qualified);
            (qualified, declared_by)
        }
        // A method of a named type: its declaration is its address.
        None => (declared.unwrap_or_default(), None),
    };
    FunctionExport {
        id,
        declared_by,
        name: name.to_string(),
        docstring: function.docstring(db).map(str::to_string),
        synthetic: name.as_str().contains('$'),
        from_default,
        signature: SignatureExport {
            generics: function
                .generic_params(db)
                .iter()
                .map(|(param, bounds)| generic_export(param, bounds))
                .collect(),
            params: sig
                .params
                .iter()
                .map(|p| ParamExport {
                    name: p
                        .name
                        .as_ref()
                        .map_or_else(|| "_".to_string(), ToString::to_string),
                    ty: TyRef::of(&p.ty),
                    optional: matches!(p.mode, baml_type::FunctionParamMode::Optional),
                })
                .collect(),
            returns: TyRef::of(&sig.return_type),
            throws: TyRef::of(&function.throws(db).effective),
        },
        source: source_export(db, function.file(db), function.span(db)),
    }
}

fn field_export(db: &dyn Db, owner: Symbol<'_>, field: Field<'_>) -> FieldExport {
    FieldExport {
        id: SymbolId::of_member(db, owner, crate::Member::Field(field))
            .map(|id| id.to_string())
            .unwrap_or_default(),
        name: field.name(db).to_string(),
        docstring: field.docstring(db).map(str::to_string),
        ty: TyRef::of(field.ty(db)),
    }
}

fn required_method_export(
    db: &dyn Db,
    owner: Symbol<'_>,
    method: RequiredMethod<'_>,
) -> RequiredMethodExport {
    let resolved = method.resolved(db);
    // Required methods have no body: the declared clause IS the contract.
    let (params, returns, throws) = match &resolved.function_ty {
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => (params.as_slice(), ret.as_ref(), throws.as_ref()),
        // A malformed required signature still exports, as unresolved.
        other => (&[][..], other, other),
    };
    RequiredMethodExport {
        id: SymbolId::of_member(db, owner, crate::Member::RequiredMethod(method))
            .map(|id| id.to_string())
            .unwrap_or_default(),
        name: method.name(db).to_string(),
        docstring: method.docstring(db).map(str::to_string),
        signature: SignatureExport {
            generics: resolved
                .generic_params
                .iter()
                .map(|(param, bounds)| generic_export(param, bounds))
                .collect(),
            params: params
                .iter()
                .map(|p| ParamExport {
                    name: p
                        .name
                        .as_ref()
                        .map_or_else(|| "_".to_string(), ToString::to_string),
                    ty: TyRef::of(&p.ty),
                    optional: matches!(p.mode, baml_type::FunctionParamMode::Optional),
                })
                .collect(),
            returns: TyRef::of(returns),
            throws: TyRef::of(throws),
        },
    }
}

/// The interface an impl names, with its arguments: `baml.ops.Add<bigint>`.
///
fn export_impl(db: &dyn Db, imp: Impl<'_>) -> Option<ImplExport> {
    let data = crate::facts::impl_data(db, imp.loc())?;
    let iface: crate::Interface<'_> = data.interface.into();
    let iface_qtn = iface.qualified_name(db);
    let pkg = baml_compiler2_hir::file_package::file_package(db, imp.file(db)).package;

    let for_ty = TyRef::of(&data.for_ty_pattern);
    // Destructured from the one renderer rather than rebuilt here, so a block's
    // id and the ids of the methods it contributes can never disagree about
    // what identifies it.
    let crate::ids::Owner::Impl {
        interface: iface_display,
        ..
    } = SymbolId::impl_owner(db, imp)?
    else {
        unreachable!("impl_owner always yields the impl form")
    };
    // No positional `#n` suffix. It used to disambiguate same-headed blocks,
    // which meant twenty ids in `baml` alone were determined by declaration
    // order — the one thing this module promises an id never depends on.
    // Reordering two `impl Add for int` blocks silently rebound every consumer
    // keyed on them. The arguments distinguish those blocks honestly, and if
    // two blocks still collide they overlap, which coherence must reject
    // rather than an id scheme paper over.
    let id = format!("X:{pkg}.impl[{iface_display} for {}]", for_ty.display);

    let mut methods: Vec<FunctionExport> = imp
        .all_methods(db)
        .into_iter()
        .map(|m| function_export(db, m.function, m.from_default, Some(imp)))
        .collect();
    methods.sort_by(|a, b| a.name.cmp(&b.name));

    Some(ImplExport {
        id,
        docstring: imp.docstring(db).map(str::to_string),
        interface: iface_qtn.render_dotted(false),
        interface_id: SymbolId::of_symbol(db, Symbol::Interface(iface))
            .map(|id| id.to_string())
            .unwrap_or_default(),
        interface_args: data.interface_args.iter().map(TyRef::of).collect(),
        for_ty,
        generics: data
            .generic_params
            .iter()
            .map(|(param, bounds)| generic_export(param, bounds))
            .collect(),
        assoc_bindings: data
            .associated_types
            .iter()
            .map(|(name, ty)| AssocBindingExport {
                name: name.to_string(),
                ty: TyRef::of(ty),
            })
            .collect(),
        methods,
        source: source_export(db, imp.file(db), imp.span(db)),
    })
}

fn export_item<'db>(
    db: &'db dyn Db,
    symbol: Symbol<'db>,
    impl_index: &ImplIndex<'db>,
) -> Option<ItemExport> {
    let id = SymbolId::of_symbol(db, symbol)?;
    let name = symbol.name(db)?;
    // An item always has a path owner; only impl-contributed methods do not,
    // and those are not exported as items.
    let namespace = match &id.owner {
        crate::ids::Owner::Path { namespace, .. } => namespace.clone(),
        crate::ids::Owner::Impl { .. } => Vec::new(),
    };

    let detail = match symbol {
        Symbol::Class(class) => {
            let mut methods: Vec<FunctionExport> = class
                .methods(db)
                .into_iter()
                .map(|m| function_export(db, m, false, None))
                .collect();
            methods.sort_by(|a, b| a.name.cmp(&b.name));
            ItemDetail::Class {
                generics: class
                    .generic_params(db)
                    .iter()
                    .map(|(param, bounds)| generic_export(param, bounds))
                    .collect(),
                fields: class
                    .fields(db)
                    .into_iter()
                    .map(|f| field_export(db, symbol, f))
                    .collect(),
                methods,
                impls: impl_index
                    .ids_for_class_head(db, &TyHead::Nominal(class.qualified_name(db))),
            }
        }
        Symbol::Enum(enm) => ItemDetail::Enum {
            variants: enm
                .variants(db)
                .into_iter()
                .map(|v: Variant<'db>| VariantExport {
                    id: SymbolId::of_member(db, symbol, crate::Member::Variant(v))
                        .map(|id| id.to_string())
                        .unwrap_or_default(),
                    name: v.name(db).to_string(),
                    docstring: v.docstring(db).map(str::to_string),
                })
                .collect(),
            impls: impl_index.ids_for_class_head(db, &TyHead::Nominal(enm.qualified_name(db))),
        },
        Symbol::Interface(iface) => {
            let mut defaults: Vec<FunctionExport> = iface
                .default_methods(db)
                .into_iter()
                .map(|m| function_export(db, m, false, None))
                .collect();
            defaults.sort_by(|a, b| a.name.cmp(&b.name));
            ItemDetail::Interface {
                generics: iface
                    .generic_params(db)
                    .iter()
                    .map(|(param, bounds)| generic_export(param, bounds))
                    .collect(),
                fields: iface
                    .fields(db)
                    .into_iter()
                    .map(|f| field_export(db, symbol, f))
                    .collect(),
                assoc_types: iface
                    .assoc_types(db)
                    .into_iter()
                    .map(|a| AssocTypeExport {
                        id: SymbolId::of_member(db, symbol, crate::Member::AssocType(a))
                            .map(|id| id.to_string())
                            .unwrap_or_default(),
                        name: a.name(db).to_string(),
                        default: a.default_ty(db).as_ref().map(TyRef::of),
                    })
                    .collect(),
                required_methods: iface
                    .required_methods(db)
                    .into_iter()
                    .map(|m| required_method_export(db, symbol, m))
                    .collect(),
                default_methods: defaults,
                implementors: impl_index.ids_for_interface(db, iface),
            }
        }
        Symbol::TypeAlias(alias) => ItemDetail::TypeAlias {
            resolved: TyRef::of(&alias.resolved(db)),
        },
        Symbol::Function(function) => ItemDetail::Function {
            signature: function_export(db, function, false, None).signature,
        },
        Symbol::TemplateString(_)
        | Symbol::Client(_)
        | Symbol::Test(_)
        | Symbol::RetryPolicy(_)
        | Symbol::Global(_) => ItemDetail::Plain {},
        Symbol::Impl(_) => return None,
    };

    Some(ItemExport {
        id: id.to_string(),
        kind: symbol.kind(),
        name: name.to_string(),
        namespace,
        docstring: symbol.docstring(db).map(str::to_string),
        synthetic: symbol.is_synthetic(db),
        source: source_export(db, symbol.file(db), symbol.span(db)),
        detail,
    })
}

// ── Single-symbol projections (describe drill-in) ───────────────────────────

/// One item plus the full records of every impl attached to it — the
/// self-contained drill-in document. Ids match `export_package`'s exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SymbolExport {
    #[serde(flatten)]
    pub item: ItemExport,
    /// Full records for the ids in the item's attachment lists.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub impl_details: Vec<ImplExport>,
}

/// Export one symbol with its attached impls inlined. `None` for symbols
/// with no id (impls themselves — export their attachment target instead).
pub fn export_symbol<'db>(db: &'db dyn Db, symbol: Symbol<'db>) -> Option<SymbolExport> {
    let impl_index = ImplIndex::build(db);
    let item = export_item(db, symbol, &impl_index)?;
    let referenced: Vec<&str> = match &item.detail {
        ItemDetail::Class { impls, .. } | ItemDetail::Enum { impls, .. } => {
            impls.iter().map(String::as_str).collect()
        }
        ItemDetail::Interface { implementors, .. } => {
            implementors.iter().map(String::as_str).collect()
        }
        ItemDetail::TypeAlias { .. } | ItemDetail::Function { .. } | ItemDetail::Plain {} => {
            Vec::new()
        }
    };
    let impl_details = impl_index
        .exports
        .into_iter()
        .map(|(_, export)| export)
        .filter(|export| referenced.contains(&export.id.as_str()))
        .collect();
    Some(SymbolExport { item, impl_details })
}

/// A member's drill-in record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "member_kind", rename_all = "snake_case")]
pub enum MemberExport {
    Method(FunctionExport),
    RequiredMethod(RequiredMethodExport),
    Field(FieldExport),
    Variant(VariantExport),
    AssocType(AssocTypeExport),
}

/// Export one member of `owner`.
pub fn export_member<'db>(
    db: &'db dyn Db,
    owner: Symbol<'db>,
    member: crate::Member<'db>,
) -> MemberExport {
    match member {
        crate::Member::Method(function) => {
            MemberExport::Method(function_export(db, function, false, None))
        }
        crate::Member::RequiredMethod(required) => {
            MemberExport::RequiredMethod(required_method_export(db, owner, required))
        }
        crate::Member::Field(field) => MemberExport::Field(field_export(db, owner, field)),
        crate::Member::Variant(variant) => MemberExport::Variant(VariantExport {
            id: SymbolId::of_member(db, owner, crate::Member::Variant(variant))
                .map(|id| id.to_string())
                .unwrap_or_default(),
            name: variant.name(db).to_string(),
            docstring: variant.docstring(db).map(str::to_string),
        }),
        crate::Member::AssocType(assoc) => MemberExport::AssocType(AssocTypeExport {
            id: SymbolId::of_member(db, owner, crate::Member::AssocType(assoc))
                .map(|id| id.to_string())
                .unwrap_or_default(),
            name: assoc.name(db).to_string(),
            default: assoc.default_ty(db).as_ref().map(TyRef::of),
        }),
    }
}
