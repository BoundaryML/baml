//! The package-surface export: one serializable document describing a
//! package's full API surface (`baml describe <pkg> --export`).
//!
//! A direct projection of compiler queries — every field corresponds to a
//! loc-keyed fact, and the whole document is deterministic for a given
//! source state, so committed artifacts diff meaningfully. Ordering is by
//! list kind: items, impls, methods, and id lists are name- or id-sorted,
//! while member lists (fields, variants, associated types, required
//! methods) keep declaration order, which is the meaningful order for a
//! reader.
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
//! The referent's full record lives in its own package's export. Note the
//! asymmetry: an item's `impls` list is an *attachment view* (project-wide),
//! while the top-level `impls` array is a *declaration set* (this package
//! only).
//!
//! The JSON is a wire contract: `tools/stdlib-matrix` decodes it (its
//! `models.baml` lists the fields it consumes), and the deployed matrix
//! ratchets on the document's content hash. This module reproduces the
//! pre-rework `baml_surface` exporter's output byte for byte; do not change
//! shapes, id spellings, or ordering without bumping [`FORMAT_VERSION`].
//! (Adding fields is non-breaking — decoders ignore undeclared fields — but
//! still moves the content hash, i.e. costs one full matrix re-judge.)
//!
//! Known completeness gaps against the semantic model, deferred to an
//! additive post-merge pass (ruled 2026-08-20): interface `requires` clauses
//! and associated-type `extends` bounds are not exported, and a bound's
//! associated-type bindings (`Iterable<Item = …>`) are dropped by
//! [`generic_export`]'s renderer (latent — no current stdlib generic bound
//! carries bindings).

use std::fmt::{self, Write as _};

use baml_base::{MediaKind, Name, SourceFile};
use baml_compiler2_hir::{
    contributions::Definition,
    loc::{ClassLoc, EnumLoc, FunctionLoc, ImplLoc, InterfaceLoc},
    namespace::NamespaceId,
    package::PackageId,
};
use baml_compiler2_ppir::item_data;
use baml_type::{
    Interface as InterfaceBound, ParamTy, PrimitiveType, QualifiedTypeName, RuntimeTy, Ty,
};
use serde::Serialize;
use text_size::TextRange;

/// Bumped on every breaking change to this schema. Consumers should check it
/// before reading anything else.
pub const FORMAT_VERSION: u32 = 1;

type Db = dyn baml_compiler2_ppir::Db;

// ── Type heads (rustdoc-style lossy impl attachment) ─────────────────────────
//
// An impl's `for` type may be generic (`implements<T extends Comparable>
// Sortable for T[]`), so attaching impls to a declaration cannot go through
// the type checker's `impls_for_type` — that path *discharges bounds*, and
// with no scope bounds registered for `T` it would silently drop every
// generic impl. Instead, an impl is attached to a declaration when their
// head constructors match, and the impl's bounds are *rendered*, not proven —
// exactly rustdoc's model for `impl<T: Ord> …`. Heads are
// companion-normalized so the structural container/primitive types and their
// builtin companion classes agree: `Ty::Int` and `class baml.Int` share the
// head `baml.Int`; `Ty::List(…)` and `class baml.Array` share `baml.Array`.

/// The head constructor of a type, for lossy impl attachment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum TyHead {
    /// A nominal head: class, enum, interface, or unexpanded alias —
    /// companion-normalized (`Ty::String` → `baml.String`, `Ty::List` →
    /// `baml.Array`, media → `baml.media.*`).
    Nominal(QualifiedTypeName),
    /// A function type — the structural function family shares one head.
    Function,
    /// A `Future` type.
    Future,
    /// A bare type variable (`implements<T> I for T`): a blanket subject that
    /// matches every declaration.
    Blanket,
}

/// The canonical companion-class name for a primitive.
fn primitive_qtn(prim: PrimitiveType) -> QualifiedTypeName {
    let path = prim.builtin_class_path();
    let (name, namespace) = path
        .split_last()
        .unwrap_or_else(|| unreachable!("builtin class paths are non-empty"));
    QualifiedTypeName::new(
        Name::new("baml"),
        namespace.iter().map(Name::new).collect(),
        Name::new(name),
    )
}

fn container_qtn(name: &str) -> QualifiedTypeName {
    QualifiedTypeName::new(Name::new("baml"), Vec::new(), Name::new(name))
}

/// Extract a type's head constructor; `None` for un-headed types (unions,
/// projections, sentinels), which no impl can attach to by head.
fn ty_head(ty: &Ty) -> Option<TyHead> {
    match ty {
        Ty::Int { .. } => Some(TyHead::Nominal(primitive_qtn(PrimitiveType::Int))),
        Ty::Bigint { .. } => Some(TyHead::Nominal(primitive_qtn(PrimitiveType::Bigint))),
        Ty::Float { .. } => Some(TyHead::Nominal(primitive_qtn(PrimitiveType::Float))),
        Ty::String { .. } => Some(TyHead::Nominal(primitive_qtn(PrimitiveType::String))),
        Ty::Bool { .. } => Some(TyHead::Nominal(primitive_qtn(PrimitiveType::Bool))),
        Ty::Null { .. } => Some(TyHead::Nominal(primitive_qtn(PrimitiveType::Null))),
        Ty::Uint8Array { .. } => Some(TyHead::Nominal(primitive_qtn(PrimitiveType::Uint8Array))),
        Ty::Media(kind, _) => match kind {
            MediaKind::Image => Some(TyHead::Nominal(primitive_qtn(PrimitiveType::Image))),
            MediaKind::Audio => Some(TyHead::Nominal(primitive_qtn(PrimitiveType::Audio))),
            MediaKind::Video => Some(TyHead::Nominal(primitive_qtn(PrimitiveType::Video))),
            MediaKind::Pdf => Some(TyHead::Nominal(primitive_qtn(PrimitiveType::Pdf))),
            // "Any media" has no single companion class.
            MediaKind::Generic => None,
        },
        Ty::Literal(lit, _, _) => Some(TyHead::Nominal(primitive_qtn(
            PrimitiveType::from_literal(lit),
        ))),
        // Companion classes (`baml.Int`, `baml.Array`, …) already carry the
        // canonical name, so nominal heads pass through unchanged.
        Ty::Class(qtn, _, _) | Ty::Interface(qtn, _, _, _) | Ty::Enum(qtn, _) => {
            Some(TyHead::Nominal(qtn.clone()))
        }
        Ty::EnumVariant(qtn, _, _) => Some(TyHead::Nominal(qtn.clone())),
        Ty::List(_, _) | Ty::EvolvingList(_, _) => Some(TyHead::Nominal(container_qtn("Array"))),
        Ty::Map { .. } | Ty::EvolvingMap(_, _, _) => Some(TyHead::Nominal(container_qtn("Map"))),
        Ty::Function { .. } => Some(TyHead::Function),
        Ty::Future(_, _, _) => Some(TyHead::Future),
        // Lossy by design: the alias head attaches without expansion.
        Ty::TypeAlias(qtn, _) => Some(TyHead::Nominal(qtn.clone())),
        Ty::TypeVar(_, _) => Some(TyHead::Blanket),
        Ty::Union(_, _) => None,
        Ty::AssociatedTypeProjection { .. } => None,
        Ty::RustType { .. }
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Void { .. } => None,
        Ty::BuiltinUnknown { .. }
        | Ty::Never { .. }
        | Ty::Unknown { .. }
        | Ty::Error { .. }
        | Ty::Infer { .. } => None,
    }
}

/// Whether an impl with `for`-head `impl_head` attaches to a declaration with
/// head `decl_head`: exact head match, or a blanket impl (which attaches to
/// everything).
fn impl_attaches(impl_head: &TyHead, decl_head: &TyHead) -> bool {
    impl_head == decl_head || *impl_head == TyHead::Blanket
}

// ── Symbol ids ───────────────────────────────────────────────────────────────
//
// A `SymbolId` names a declaration by *what it is* — package, namespace path,
// name, kind, optional member — never by an interning order or a file offset,
// so ids survive across compilations and processes. The string form uses a
// single-letter kind prefix because BAML's type and value namespaces are
// distinct (`class Foo` and `function Foo` can coexist):
//
// ```text
// T:baml.time.Duration              type-space item
// V:baml.json.parse                 value-space item
// M:baml.time.Duration.abs          method
// F:user.Point.x                    field
// E:user.Color.Red                  enum variant
// A:baml.Comparable.CompareError    associated type
// M:(int as baml.ops.Add<bigint>).add   method reached through an impl block
// ```
//
// The interface's arguments are load-bearing in the impl form: one type may
// implement one interface at several instantiations (multi-RHS operator
// overloading), and each contributes a method under the same name.
// Qualification is unconditional rather than applied on collision, so an id
// never changes because an unrelated impl appeared.

/// The kind discriminant of a [`SymbolId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum IdKind {
    /// Type-space item: class, enum, interface, type alias.
    Type,
    /// Value-space item: function, global, client, test, ….
    Value,
    /// A method member (class method, interface default, or required).
    Method,
    /// A field member of a class or interface.
    Field,
    /// An enum variant.
    Variant,
    /// An associated type of an interface.
    AssocType,
}

impl IdKind {
    fn prefix(self) -> char {
        match self {
            Self::Type => 'T',
            Self::Value => 'V',
            Self::Method => 'M',
            Self::Field => 'F',
            Self::Variant => 'E',
            Self::AssocType => 'A',
        }
    }
}

/// What an id hangs off: a named item reached by path, or an impl block
/// (`(int as baml.ops.Add<bigint>)`) — the block is not a path, since neither
/// half of it need be addressable as an item.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Owner {
    Path {
        package: String,
        namespace: Vec<String>,
        /// The item's name; for a member id, the *containing type's* name.
        name: String,
    },
    Impl {
        /// The implementing type, canonically rendered: `int`.
        for_ty: String,
        /// The interface with its arguments: `baml.ops.Add<bigint>`.
        interface: String,
    },
}

impl fmt::Display for Owner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path {
                package,
                namespace,
                name,
            } => {
                write!(f, "{package}")?;
                for seg in namespace {
                    write!(f, ".{seg}")?;
                }
                write!(f, ".{name}")
            }
            Self::Impl { for_ty, interface } => write!(f, "({for_ty} as {interface})"),
        }
    }
}

/// A stable, content-derived symbol identity — see the section comment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SymbolId {
    kind: IdKind,
    owner: Owner,
    /// The member's name, for member kinds; `None` for item kinds.
    member: Option<String>,
}

impl fmt::Display for SymbolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind.prefix(), self.owner)?;
        if let Some(member) = &self.member {
            write!(f, ".{member}")?;
        }
        Ok(())
    }
}

impl SymbolId {
    /// The id of a top-level item. A method's id nests under its owner type
    /// (or its contributing impl block) rather than the namespace.
    fn of_definition(db: &Db, def: Definition<'_>) -> Option<Self> {
        if let Definition::Function(func) = def {
            // An impl-contributed method is addressed through its block, even
            // when the block is written in the class body and the method is
            // therefore also a class method: `Duration` implementing both
            // `Multiply<int>` and `Multiply<bigint>` contributes two methods
            // named `mul`, and `M:baml.time.Duration.mul` cannot name both.
            if let Some(imp) = contributing_impl(db, func) {
                return Some(Self {
                    kind: IdKind::Method,
                    owner: Self::impl_owner(db, imp)?,
                    member: Some(function_name(db, func).to_string()),
                });
            }
            match item_data::method_owner(db, func) {
                Some(item_data::MethodOwner::Class(class)) => {
                    return Some(Self::member_id(
                        db,
                        class.file(db),
                        &item_data::class_data(db, class).name,
                        IdKind::Method,
                        &function_name(db, func),
                    ));
                }
                Some(item_data::MethodOwner::Interface(iface)) => {
                    return Some(Self::member_id(
                        db,
                        iface.file(db),
                        &item_data::interface_data(db, iface).name,
                        IdKind::Method,
                        &function_name(db, func),
                    ));
                }
                Some(item_data::MethodOwner::FreeImpl(_)) | None => {}
            }
        }

        let kind = match def {
            Definition::Class(_)
            | Definition::Enum(_)
            | Definition::Interface(_)
            | Definition::TypeAlias(_) => IdKind::Type,
            Definition::Function(_)
            | Definition::TemplateString(_)
            | Definition::Client(_)
            | Definition::Test(_)
            | Definition::RetryPolicy(_)
            | Definition::Let(_) => IdKind::Value,
        };
        let name = definition_name(db, def);
        let pkg = baml_compiler2_hir::file_package::file_package(db, definition_file(db, def));
        Some(Self {
            kind,
            owner: Owner::Path {
                package: pkg.package.to_string(),
                namespace: pkg.namespace_path.iter().map(ToString::to_string).collect(),
                name: name.to_string(),
            },
            member: None,
        })
    }

    /// How an impl block is written inside an id:
    /// `(int as baml.ops.Add<bigint>)`. The one renderer, shared with the
    /// block ids, so the two can never disagree about what identifies an impl.
    fn impl_owner(db: &Db, imp: ImplLoc<'_>) -> Option<Owner> {
        let data = impl_facts(db, imp)?;
        let mut interface = interface_qtn(db, data.interface).render_dotted(false);
        if !data.interface_args.is_empty() {
            let args: Vec<String> = data
                .interface_args
                .iter()
                .map(Ty::render_canonical)
                .collect();
            interface = format!("{interface}<{}>", args.join(", "));
        }
        Some(Owner::Impl {
            for_ty: data.for_ty_pattern.render_canonical(),
            interface,
        })
    }

    /// The id of a member of the named item declared in `owner_file`.
    fn member_id(
        db: &Db,
        owner_file: SourceFile,
        owner_name: &Name,
        kind: IdKind,
        member: &Name,
    ) -> Self {
        let pkg = baml_compiler2_hir::file_package::file_package(db, owner_file);
        Self {
            kind,
            owner: Owner::Path {
                package: pkg.package.to_string(),
                namespace: pkg.namespace_path.iter().map(ToString::to_string).collect(),
                name: owner_name.to_string(),
            },
            member: Some(member.to_string()),
        }
    }
}

/// The impl block that contributes `function`, if any: the block itself for
/// free-impl methods, or the class's (in-body/merged) block that lists it.
fn contributing_impl<'db>(db: &'db Db, function: FunctionLoc<'db>) -> Option<ImplLoc<'db>> {
    match item_data::method_owner(db, function)? {
        item_data::MethodOwner::FreeImpl(imp) => Some(imp),
        item_data::MethodOwner::Class(class) => item_data::class_impls(db, class)
            .iter()
            .copied()
            .find(|&imp| {
                item_data::impl_block_data(db, imp)
                    .methods
                    .contains(&function)
            }),
        item_data::MethodOwner::Interface(_) => None,
    }
}

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
    /// `unknown` leaf. Consumers must not treat the display string as a real
    /// type. Deliberately-symbolic forms (`(Self as I).Member`, free type
    /// variables) are *not* flagged: they are the correct declaration-site
    /// types.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unresolved: bool,
}

impl TyRef {
    fn of(ty: &Ty) -> Self {
        let head = match ty_head(ty) {
            Some(TyHead::Nominal(qtn)) => Some(
                SymbolId {
                    kind: IdKind::Type,
                    owner: Owner::Path {
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
            display: ty.render_canonical(),
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
                    let args: Vec<String> = b.generics.iter().map(Ty::render_canonical).collect();
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
    /// raise, not whether the author wrote it down.
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
    /// addressed *through* it — `M:(int as baml.ops.Add<bigint>).add` —
    /// because the same declaration can be re-listed by many blocks and each
    /// listing needs its own address.
    pub id: String,
    /// Where the code is written, when that is somewhere else — an inherited
    /// default names the interface method it came from. Absent when `id`
    /// already names the declaration.
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
    /// Structural id: `X:<pkg>.impl[<interface> for <for-type>]`. No
    /// positional suffix: the interface arguments distinguish same-headed
    /// blocks honestly, and two blocks that still collide overlap, which
    /// coherence must reject rather than an id scheme paper over.
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

/// The structural kind of an exported item, serialized exactly as the
/// pre-rework surface layer spelled it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportItemKind {
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
}

fn item_kind(def: Definition<'_>) -> ExportItemKind {
    match def {
        Definition::Class(_) => ExportItemKind::Class,
        Definition::Enum(_) => ExportItemKind::Enum,
        Definition::Interface(_) => ExportItemKind::Interface,
        Definition::TypeAlias(_) => ExportItemKind::TypeAlias,
        Definition::Function(_) => ExportItemKind::Function,
        Definition::TemplateString(_) => ExportItemKind::TemplateString,
        Definition::Client(_) => ExportItemKind::Client,
        Definition::Test(_) => ExportItemKind::Test,
        Definition::RetryPolicy(_) => ExportItemKind::RetryPolicy,
        Definition::Let(_) => ExportItemKind::Global,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemExport {
    pub id: String,
    pub kind: ExportItemKind,
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

// ── Fact accessors ───────────────────────────────────────────────────────────

/// Resolved impl header and members. `None` when the block is malformed
/// (unresolvable interface target, cyclic header) — broken impls carry
/// diagnostics through the check paths and are omitted from surface listings.
fn impl_facts<'db>(
    db: &'db Db,
    imp: ImplLoc<'db>,
) -> Option<&'db baml_compiler2_hir_ty::interfaces::ImplData<'db>> {
    baml_compiler2_hir_ty::interfaces::impl_data(db, imp)
        .as_ref()
        .ok()
}

fn function_name(db: &Db, func: FunctionLoc<'_>) -> Name {
    item_data::function_data(db, func).name.clone()
}

fn interface_qtn(db: &Db, iface: InterfaceLoc<'_>) -> QualifiedTypeName {
    let pkg = baml_compiler2_hir::file_package::file_package(db, iface.file(db));
    QualifiedTypeName::new(
        pkg.package,
        pkg.namespace_path,
        item_data::interface_data(db, iface).name.clone(),
    )
}

fn class_qtn(db: &Db, class: ClassLoc<'_>) -> QualifiedTypeName {
    let pkg = baml_compiler2_hir::file_package::file_package(db, class.file(db));
    QualifiedTypeName::new(
        pkg.package,
        pkg.namespace_path,
        item_data::class_data(db, class).name.clone(),
    )
}

fn enum_qtn(db: &Db, enm: EnumLoc<'_>) -> QualifiedTypeName {
    let pkg = baml_compiler2_hir::file_package::file_package(db, enm.file(db));
    QualifiedTypeName::new(
        pkg.package,
        pkg.namespace_path,
        item_data::enum_data(db, enm).name.clone(),
    )
}

fn definition_name(db: &Db, def: Definition<'_>) -> Name {
    match def {
        Definition::Class(loc) => item_data::class_data(db, loc).name.clone(),
        Definition::Enum(loc) => item_data::enum_data(db, loc).name.clone(),
        Definition::Interface(loc) => item_data::interface_data(db, loc).name.clone(),
        Definition::TypeAlias(loc) => item_data::type_alias_data(db, loc).name.clone(),
        Definition::Function(loc) => item_data::function_data(db, loc).name.clone(),
        Definition::TemplateString(loc) => item_data::template_string_data(db, loc).name.clone(),
        Definition::Client(loc) => item_data::client_data(db, loc).name.clone(),
        Definition::Test(loc) => item_data::test_data(db, loc).name.clone(),
        Definition::RetryPolicy(loc) => item_data::retry_policy_data(db, loc).name.clone(),
        Definition::Let(loc) => item_data::let_data(db, loc).name.clone(),
    }
}

fn definition_file(db: &Db, def: Definition<'_>) -> SourceFile {
    match def {
        Definition::Class(loc) => loc.file(db),
        Definition::Enum(loc) => loc.file(db),
        Definition::Interface(loc) => loc.file(db),
        Definition::TypeAlias(loc) => loc.file(db),
        Definition::Function(loc) => loc.file(db),
        Definition::TemplateString(loc) => loc.file(db),
        Definition::Client(loc) => loc.file(db),
        Definition::Test(loc) => loc.file(db),
        Definition::RetryPolicy(loc) => loc.file(db),
        Definition::Let(loc) => loc.file(db),
    }
}

fn definition_span(db: &Db, def: Definition<'_>) -> TextRange {
    match def {
        Definition::Class(loc) => item_data::class_source_map(db, loc).span,
        Definition::Enum(loc) => item_data::enum_source_map(db, loc).span,
        Definition::Interface(loc) => item_data::interface_source_map(db, loc).span,
        Definition::TypeAlias(loc) => item_data::type_alias_source_map(db, loc).span,
        Definition::Function(loc) => item_data::function_source_map(db, loc).span,
        Definition::TemplateString(loc) => item_data::template_string_source_map(db, loc).span,
        Definition::Client(loc) => item_data::client_source_map(db, loc).span,
        Definition::Test(loc) => item_data::test_source_map(db, loc).span,
        Definition::RetryPolicy(loc) => item_data::retry_policy_source_map(db, loc).span,
        Definition::Let(loc) => item_data::let_source_map(db, loc).span,
    }
}

/// The leading `///` docstring, where the kind carries one. Template
/// strings, clients, tests, retry policies, and globals carry none in the
/// item data today.
fn definition_docstring<'db>(db: &'db Db, def: Definition<'db>) -> Option<&'db str> {
    match def {
        Definition::Class(loc) => item_data::class_data(db, loc).docstring.as_deref(),
        Definition::Enum(loc) => item_data::enum_data(db, loc).docstring.as_deref(),
        Definition::Interface(loc) => item_data::interface_data(db, loc).docstring.as_deref(),
        Definition::TypeAlias(loc) => item_data::type_alias_data(db, loc).docstring.as_deref(),
        Definition::Function(loc) => item_data::function_data(db, loc).docstring.as_deref(),
        Definition::TemplateString(_)
        | Definition::Client(_)
        | Definition::Test(_)
        | Definition::RetryPolicy(_)
        | Definition::Let(_) => None,
    }
}

/// A function's own generic parameters with their resolved interface bounds,
/// in declaration order. The bounds map is sparse — an unbounded parameter
/// has no entry and gets an empty conjunction.
fn function_generics(db: &Db, func: FunctionLoc<'_>) -> Vec<(ParamTy, Vec<InterfaceBound>)> {
    let bounds = plain_bounds(baml_compiler2_hir_ty::lower::function_generic_bounds(
        db, func,
    ));
    baml_compiler2_hir_ty::callable::function_signature_ty(db, func)
        .generic_params
        .iter()
        .map(|param| {
            (
                param.clone(),
                bounds
                    .iter()
                    .find(|(p, _)| p == param)
                    .map(|(_, b)| b.clone())
                    .unwrap_or_default(),
            )
        })
        .collect()
}

fn class_generics(db: &Db, class: ClassLoc<'_>) -> Vec<(ParamTy, Vec<InterfaceBound>)> {
    let bounds = plain_bounds(baml_compiler2_hir_ty::lower::class_generic_bounds(
        db, class,
    ));
    baml_compiler2_hir_ty::lower::class_generic_frame(db, class)
        .into_iter()
        .map(|param| {
            let ifaces = bounds
                .iter()
                .find(|(p, _)| *p == param)
                .map(|(_, b)| b.clone())
                .unwrap_or_default();
            (param, ifaces)
        })
        .collect()
}

/// The parameters the interface *declares* — not the in-scope view, which
/// would lead with the implicit `Self`.
fn interface_generics(db: &Db, iface: InterfaceLoc<'_>) -> Vec<(ParamTy, Vec<InterfaceBound>)> {
    let bounds = baml_compiler2_hir_ty::interfaces::interface_declared_param_bounds(db, iface);
    baml_compiler2_hir_ty::lower::interface_declared_params(db, iface)
        .into_iter()
        .map(|param| {
            let ifaces = bounds.get(&param).cloned().unwrap_or_default();
            (param, ifaces)
        })
        .collect()
}

/// An interned bounds map (the lowering layer's shape) as a plain list.
fn plain_bounds(
    interned: impl IntoIterator<Item = (ParamTy, Vec<baml_type::interned::InterfaceRef>)>,
) -> Vec<(ParamTy, Vec<InterfaceBound>)> {
    interned
        .into_iter()
        .map(|(param, refs)| {
            (
                param,
                refs.iter()
                    .map(|bound| InterfaceBound {
                        name: bound.name.clone(),
                        generics: bound
                            .generics
                            .iter()
                            .map(baml_type::interned::Ty::to_plain)
                            .collect(),
                        associated_types: bound
                            .associated_types
                            .iter()
                            .map(|(name, t)| (name.clone(), t.to_plain()))
                            .collect(),
                    })
                    .collect(),
            )
        })
        .collect()
}

// ── Projection ───────────────────────────────────────────────────────────────

/// Export one package's full surface.
pub fn export_package<'db>(db: &'db Db, package: PackageId<'db>) -> PackageExport {
    let impl_index = ImplIndex::build(db);

    // Namespaces root-first sorted by path; items types-then-values sorted
    // by name within each namespace. (The final id sort makes the walk order
    // invisible in the artifact; it is kept for deterministic tie behavior.)
    let items_index = baml_compiler2_ppir::package_items(db, package);
    let mut ns_paths: Vec<&Vec<Name>> = items_index.namespaces.keys().collect();
    ns_paths.sort();

    let mut items = Vec::new();
    for path in ns_paths {
        let ns = NamespaceId::new(db, package.name(db), path.clone());
        let ns_items = baml_compiler2_ppir::namespace_items(db, ns);
        let mut named: Vec<(&Name, Definition<'db>)> = ns_items
            .types
            .iter()
            .map(|(name, def)| (name, *def))
            .collect();
        named.sort_by(|(a, _), (b, _)| a.cmp(b));
        let mut values: Vec<(&Name, Definition<'db>)> = ns_items
            .values
            .iter()
            .map(|(name, def)| (name, *def))
            .collect();
        values.sort_by(|(a, _), (b, _)| a.cmp(b));
        named.extend(values);
        for (_, def) in named {
            if let Some(item) = export_item(db, def, &impl_index) {
                items.push(item);
            }
        }
    }
    items.sort_by(|a, b| a.id.cmp(&b.id));

    let package_name = package.name(db);
    let mut impls: Vec<ImplExport> = impl_index
        .exports
        .into_iter()
        .filter(|(imp, _)| {
            baml_compiler2_hir::file_package::file_package(db, imp.file(db)).package == package_name
        })
        .map(|(_, export)| export)
        .collect();
    impls.sort_by(|a, b| a.id.cmp(&b.id));

    PackageExport {
        format_version: FORMAT_VERSION,
        package: package_name.to_string(),
        items,
        impls,
    }
}

/// Every impl block in the project — user package and builtins alike, in
/// file order. Impl attachment must see the whole project because the orphan
/// rule allows an impl to live downstream of the type it implements
/// (`implements MyIface for int` in user code attaches to `baml.Int`).
struct ImplIndex<'db> {
    exports: Vec<(ImplLoc<'db>, ImplExport)>,
}

impl<'db> ImplIndex<'db> {
    fn build(db: &'db Db) -> Self {
        let mut exports = Vec::new();
        for file in baml_compiler2_hir::compiler2_all_files(db) {
            for &imp in item_data::file_impls(db, file) {
                if let Some(export) = export_impl(db, imp) {
                    exports.push((imp, export));
                }
            }
        }
        Self { exports }
    }

    fn ids_for_class_head(&self, db: &Db, head: &TyHead) -> Vec<String> {
        let mut ids: Vec<String> = self
            .exports
            .iter()
            .filter(|(imp, _)| {
                impl_facts(db, *imp)
                    .and_then(|data| ty_head(&data.for_ty_pattern))
                    .is_some_and(|impl_head| impl_attaches(&impl_head, head))
            })
            .map(|(_, export)| export.id.clone())
            .collect();
        ids.sort();
        ids
    }

    fn ids_for_interface(&self, db: &Db, iface: InterfaceLoc<'_>) -> Vec<String> {
        let mut ids: Vec<String> = self
            .exports
            .iter()
            .filter(|(imp, _)| impl_facts(db, *imp).is_some_and(|data| data.interface == iface))
            .map(|(_, export)| export.id.clone())
            .collect();
        ids.sort();
        ids
    }
}

fn source_export(db: &Db, file: SourceFile, span: TextRange) -> SourceExport {
    SourceExport {
        file: file.path(db).to_string_lossy().into_owned(),
        start: span.start().into(),
        end: span.end().into(),
    }
}

/// One function record. `via` is the impl block this entry is listed under,
/// when it is listed under one.
///
/// An impl entry is addressed under its block rather than by the declaring
/// symbol's id: an inherited default is re-listed by every implementor, and
/// a method declared in a free impl has no symbol id at all. The declaring
/// id is kept in `declared_by`, which is what a consumer dedupes on when it
/// wants to treat one declaration as one thing.
fn function_export(
    db: &Db,
    function: FunctionLoc<'_>,
    from_default: bool,
    via: Option<ImplLoc<'_>>,
) -> FunctionExport {
    let sig = baml_compiler2_hir_ty::callable::function_signature_ty(db, function);
    let data = item_data::function_data(db, function);
    let name = data.name.clone();
    let declared =
        SymbolId::of_definition(db, Definition::Function(function)).map(|id| id.to_string());
    let (id, declared_by) = match via {
        // Reached through an impl block, so addressed through it, in the
        // qualified path a caller would actually write.
        Some(imp) => {
            let qualified = SymbolId::impl_owner(db, imp)
                .map(|owner| {
                    SymbolId {
                        kind: IdKind::Method,
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
    let source_map = item_data::function_source_map(db, function);
    FunctionExport {
        id,
        declared_by,
        name: name.to_string(),
        docstring: data.docstring.clone(),
        synthetic: name.as_str().contains('$'),
        from_default,
        signature: SignatureExport {
            generics: function_generics(db, function)
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
            throws: TyRef::of(&baml_compiler2_hir_ty::callable::callable_throws(db, function).0),
        },
        source: source_export(db, function.file(db), source_map.span),
    }
}

fn class_field_export(db: &Db, class: ClassLoc<'_>, index: usize) -> FieldExport {
    let data = item_data::class_data(db, class);
    let field = &data.fields[index];
    FieldExport {
        id: SymbolId::member_id(db, class.file(db), &data.name, IdKind::Field, &field.name)
            .to_string(),
        name: field.name.to_string(),
        docstring: field.docstring.clone(),
        ty: TyRef::of(&baml_compiler2_hir_ty::lower::resolve_class_fields(db, class)[index].1),
    }
}

fn interface_field_export(db: &Db, iface: InterfaceLoc<'_>, index: usize) -> FieldExport {
    let data = item_data::interface_data(db, iface);
    let field = &data.fields[index];
    FieldExport {
        id: SymbolId::member_id(db, iface.file(db), &data.name, IdKind::Field, &field.name)
            .to_string(),
        name: field.name.to_string(),
        docstring: field.docstring.clone(),
        ty: TyRef::of(
            &baml_compiler2_hir_ty::interfaces::resolve_interface_fields(db, iface).fields[index].1,
        ),
    }
}

fn required_method_export(db: &Db, iface: InterfaceLoc<'_>, index: usize) -> RequiredMethodExport {
    let data = item_data::interface_data(db, iface);
    let declared = &data.required_methods[index];
    let resolved =
        &baml_compiler2_hir_ty::interfaces::resolve_interface_required_methods(db, iface)[index];
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
        id: SymbolId::member_id(
            db,
            iface.file(db),
            &data.name,
            IdKind::Method,
            &declared.name,
        )
        .to_string(),
        name: declared.name.to_string(),
        docstring: declared.docstring.clone(),
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

fn export_impl(db: &Db, imp: ImplLoc<'_>) -> Option<ImplExport> {
    let data = impl_facts(db, imp)?;
    let iface_qtn = interface_qtn(db, data.interface);
    let pkg = baml_compiler2_hir::file_package::file_package(db, imp.file(db)).package;

    let for_ty = TyRef::of(&data.for_ty_pattern);
    // Destructured from the one renderer rather than rebuilt here, so a
    // block's id and the ids of the methods it contributes can never disagree
    // about what identifies it.
    let Owner::Impl {
        interface: iface_display,
        ..
    } = SymbolId::impl_owner(db, imp)?
    else {
        unreachable!("impl_owner always yields the impl form")
    };
    let id = format!("X:{pkg}.impl[{iface_display} for {}]", for_ty.display);

    // Every method the impl supplies, rustdoc-style: the block's own
    // overrides plus the interface's default methods it did not override.
    let mut methods: Vec<FunctionExport> = data
        .methods
        .iter()
        .map(|&loc| function_export(db, loc, false, Some(imp)))
        .collect();
    let overridden: Vec<String> = methods.iter().map(|m| m.name.clone()).collect();
    for &default_loc in &item_data::interface_data(db, data.interface).default_methods {
        if !overridden.contains(&function_name(db, default_loc).to_string()) {
            methods.push(function_export(db, default_loc, true, Some(imp)));
        }
    }
    methods.sort_by(|a, b| a.name.cmp(&b.name));

    let block_data = item_data::impl_block_data(db, imp);
    Some(ImplExport {
        id,
        docstring: block_data.docstring.clone(),
        interface: iface_qtn.render_dotted(false),
        interface_id: SymbolId::of_definition(db, Definition::Interface(data.interface))
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
        source: source_export(
            db,
            imp.file(db),
            item_data::impl_block_source_map(db, imp).span,
        ),
    })
}

fn export_item<'db>(
    db: &'db Db,
    def: Definition<'db>,
    impl_index: &ImplIndex<'db>,
) -> Option<ItemExport> {
    let id = SymbolId::of_definition(db, def)?;
    let name = definition_name(db, def);
    // An item always has a path owner; only impl-contributed methods do not,
    // and those are not exported as items.
    let namespace = match &id.owner {
        Owner::Path { namespace, .. } => namespace.clone(),
        Owner::Impl { .. } => Vec::new(),
    };

    let detail = match def {
        Definition::Class(class) => {
            let data = item_data::class_data(db, class);
            let mut methods: Vec<FunctionExport> = data
                .methods
                .iter()
                .map(|&loc| function_export(db, loc, false, None))
                .collect();
            methods.sort_by(|a, b| a.name.cmp(&b.name));
            ItemDetail::Class {
                generics: class_generics(db, class)
                    .iter()
                    .map(|(param, bounds)| generic_export(param, bounds))
                    .collect(),
                fields: (0..data.fields.len())
                    .map(|index| class_field_export(db, class, index))
                    .collect(),
                methods,
                impls: impl_index.ids_for_class_head(db, &TyHead::Nominal(class_qtn(db, class))),
            }
        }
        Definition::Enum(enm) => {
            let data = item_data::enum_data(db, enm);
            ItemDetail::Enum {
                variants: data
                    .variants
                    .iter()
                    .map(|variant| VariantExport {
                        id: SymbolId::member_id(
                            db,
                            enm.file(db),
                            &data.name,
                            IdKind::Variant,
                            &variant.name,
                        )
                        .to_string(),
                        name: variant.name.to_string(),
                        docstring: variant.docstring.clone(),
                    })
                    .collect(),
                impls: impl_index.ids_for_class_head(db, &TyHead::Nominal(enum_qtn(db, enm))),
            }
        }
        Definition::Interface(iface) => {
            let data = item_data::interface_data(db, iface);
            let mut defaults: Vec<FunctionExport> = data
                .default_methods
                .iter()
                .map(|&loc| function_export(db, loc, false, None))
                .collect();
            defaults.sort_by(|a, b| a.name.cmp(&b.name));
            ItemDetail::Interface {
                generics: interface_generics(db, iface)
                    .iter()
                    .map(|(param, bounds)| generic_export(param, bounds))
                    .collect(),
                fields: (0..data.fields.len())
                    .map(|index| interface_field_export(db, iface, index))
                    .collect(),
                assoc_types: data
                    .associated_types
                    .iter()
                    .map(|assoc| AssocTypeExport {
                        id: SymbolId::member_id(
                            db,
                            iface.file(db),
                            &data.name,
                            IdKind::AssocType,
                            &assoc.name,
                        )
                        .to_string(),
                        name: assoc.name.to_string(),
                        default:
                            baml_compiler2_hir_ty::interfaces::interface_associated_type_default(
                                db,
                                iface,
                                assoc.name.clone(),
                            )
                            .map(|(ty, _diags)| TyRef::of(&ty)),
                    })
                    .collect(),
                required_methods: (0..data.required_methods.len())
                    .map(|index| required_method_export(db, iface, index))
                    .collect(),
                default_methods: defaults,
                implementors: impl_index.ids_for_interface(db, iface),
            }
        }
        Definition::TypeAlias(alias) => ItemDetail::TypeAlias {
            resolved: TyRef::of(
                &baml_compiler2_hir_ty::lower::type_alias_value(db, alias).to_plain(),
            ),
        },
        Definition::Function(function) => ItemDetail::Function {
            signature: function_export(db, function, false, None).signature,
        },
        Definition::TemplateString(_)
        | Definition::Client(_)
        | Definition::Test(_)
        | Definition::RetryPolicy(_)
        | Definition::Let(_) => ItemDetail::Plain {},
    };

    Some(ItemExport {
        id: id.to_string(),
        kind: item_kind(def),
        name: name.to_string(),
        namespace,
        docstring: definition_docstring(db, def).map(str::to_string),
        // Reliable, not heuristic: `$` cannot appear in a user identifier,
        // and every compiler-synthesized top-level item is `$`-named.
        synthetic: name.as_str().contains('$'),
        source: source_export(db, definition_file(db, def), definition_span(db, def)),
        detail,
    })
}

#[cfg(test)]
mod tests {
    use baml_db::ProjectDatabase;

    use super::*;

    fn make_db() -> ProjectDatabase {
        let mut db = ProjectDatabase::new();
        db.ensure_stdlib_sources();
        db
    }

    fn package<'db>(db: &'db ProjectDatabase, name: &str) -> PackageId<'db> {
        PackageId::new(db, Name::new(name))
    }

    /// The whole `assert` package, pretty-printed — small enough to review,
    /// and it exercises functions, signatures, and throws end to end.
    #[test]
    fn assert_package_exports_fully() {
        let db = make_db();
        let export = export_package(&db, package(&db, "assert"));
        // Without this the test still passes if `assert` stops resolving: the
        // export becomes an empty document and the snapshot is accepted as-is.
        assert!(
            !export.items.is_empty(),
            "the assert package resolves and exports items"
        );
        insta::assert_snapshot!(serde_json::to_string_pretty(&export).unwrap());
    }

    /// Every `id` in the document addresses exactly one *symbol*.
    ///
    /// Consumers key on ids — a report diffs on them, a cache blesses on
    /// them — so a collision is not a cosmetic flaw but a wrong answer about
    /// a different symbol. The pressure is entirely on impl blocks: an
    /// inherited default is re-listed by every implementor, which is why an
    /// impl entry is addressed through its block and keeps the declaration
    /// in `declared_by`.
    ///
    /// One symbol may legitimately appear twice, and the invariant is stated
    /// on records rather than on strings for that reason: a method written
    /// in a class body's `implements` block is both a method of the class
    /// and a method of the block, and both views list it. What must never
    /// happen is one id covering two *different* records, so equal ids are
    /// required to carry equal content.
    #[test]
    fn every_exported_id_is_unique() {
        let db = make_db();
        let json = serde_json::to_value(export_package(&db, package(&db, "baml"))).unwrap();

        let mut ids: Vec<String> = Vec::new();
        let mut records: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();
        let mut conflicts: Vec<String> = Vec::new();
        let mut collect = |value: &serde_json::Value| {
            if let Some(id) = value.get("id").and_then(serde_json::Value::as_str) {
                ids.push(id.to_string());
                match records.get(id) {
                    Some(existing) if existing != value => conflicts.push(id.to_string()),
                    Some(_) => {}
                    None => {
                        records.insert(id.to_string(), value.clone());
                    }
                }
            }
        };
        for item in json["items"].as_array().unwrap() {
            collect(item);
            for key in [
                "fields",
                "methods",
                "variants",
                "assoc_types",
                "required_methods",
                // An interface's defaults are a member list like any other,
                // and every entry carries an id. Omitting the key left a
                // whole class of member outside the invariant this test
                // exists to state.
                "default_methods",
            ] {
                for member in item
                    .get(key)
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    collect(member);
                }
            }
        }
        for block in json["impls"].as_array().unwrap() {
            collect(block);
            for method in block["methods"].as_array().unwrap() {
                collect(method);
            }
        }

        assert!(ids.len() > 1000, "the census actually walked the document");
        conflicts.sort();
        conflicts.dedup();
        assert!(
            conflicts.is_empty(),
            "{} id(s) cover more than one record, e.g. {:?}",
            conflicts.len(),
            &conflicts[..conflicts.len().min(5)]
        );

        // No `::` anywhere: it is Rust's path separator and appears nowhere
        // in BAML's grammar, so an id containing one can be neither pasted
        // into `describe` nor written in source.
        let rustish: Vec<&String> = ids.iter().filter(|id| id.contains("::")).collect();
        assert!(
            rustish.is_empty(),
            "{} id(s) are spelled with `::`, e.g. {:?}",
            rustish.len(),
            &rustish[..rustish.len().min(5)]
        );
    }

    #[test]
    fn export_is_byte_deterministic() {
        let db = make_db();
        let a = serde_json::to_string(&export_package(&db, package(&db, "baml"))).unwrap();
        let b = serde_json::to_string(&export_package(&db, package(&db, "baml"))).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn baml_package_export_cross_links() {
        let db = make_db();
        let export = export_package(&db, package(&db, "baml"));
        let json = serde_json::to_value(&export).unwrap();
        let items = json["items"].as_array().unwrap();

        let find = |id: &str| {
            items
                .iter()
                .find(|item| item["id"] == id)
                .unwrap_or_else(|| panic!("missing item {id}"))
        };

        // Cross-link: baml.Int's impl list includes the Comparable block,
        // and that block's export carries `compare`.
        let int = find("T:baml.Int");
        let int_impls: Vec<&str> = int["impls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let comparable_impl = int_impls
            .iter()
            .find(|id| id.contains("baml.Comparable for int"))
            .unwrap_or_else(|| panic!("Int lists its Comparable impl: {int_impls:?}"));
        let impls = json["impls"].as_array().unwrap();
        let block = impls
            .iter()
            .find(|imp| imp["id"] == **comparable_impl)
            .expect("Comparable-for-int block is exported");
        assert!(
            block["methods"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["name"] == "compare"),
            "compare is listed"
        );

        // The generic Sortable impl attaches to Array with its symbolic
        // binding.
        let array = find("T:baml.Array");
        let array_impls: Vec<&str> = array["impls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(
            array_impls
                .iter()
                .any(|id| id.contains("baml.Sortable for T[]")),
            "Sortable attaches to Array: {array_impls:?}"
        );

        // Interface records list their implementors.
        let comparable = find("T:baml.Comparable");
        assert!(
            comparable["implementors"].as_array().unwrap().len() >= 4,
            "Comparable lists implementors"
        );
        // Required-method signature: Self stays symbolic in the export.
        let required = comparable["required_methods"].as_array().unwrap();
        let compare = required
            .iter()
            .find(|m| m["name"] == "compare")
            .expect("Comparable::compare is required");
        assert_eq!(
            compare["signature"]["throws"]["display"],
            "(Self as baml.Comparable).CompareError"
        );

        // An interface exports the parameters it declares, and only those.
        // The in-scope view leads with the implicit `Self`, which belongs to
        // every interface and so describes none of them; exporting it would
        // read as `interface Add<Self, Rhs>`.
        let add = find("T:baml.ops.Add");
        let add_generics: Vec<&str> = add["generics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["name"].as_str().unwrap())
            .collect();
        assert_eq!(add_generics, ["Rhs"], "Add declares Rhs alone");
        assert_eq!(
            add["generics"][0]["bounds"][0], "baml.Concrete",
            "the parameter's bound comes with it"
        );
        assert!(
            comparable["generics"].as_array().is_none_or(Vec::is_empty),
            "an interface with no declared parameters exports none"
        );
        // Associated types are exported as members of the interface that
        // owns them.
        let sortable = find("T:baml.Sortable");
        assert!(
            sortable["assoc_types"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["name"] == "SortError"),
            "Sortable carries SortError"
        );

        // Synthetic companions are present and flagged, never dropped.
        assert!(
            items.iter().any(|item| item["synthetic"] == true
                && item["id"].as_str().unwrap().contains("$stream")),
            "synthetic $stream companions are listed and flagged"
        );

        // Docstrings survive.
        let string = find("T:baml.String");
        assert!(
            string["docstring"]
                .as_str()
                .unwrap()
                .contains("UTF-8 encoded string")
        );
    }
}
