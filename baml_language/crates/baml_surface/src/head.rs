//! Head-constructor extraction over [`Ty`], for rustdoc-style lossy impl
//! attachment.
//!
//! An impl's `for` type may be generic (`implements<T extends Comparable>
//! Sortable for T[]`), so attaching impls to a declaration cannot go through
//! the type checker's `impls_for_type` — that path *discharges bounds*, and
//! with no scope bounds registered for `T` it would silently drop every
//! generic impl. Instead, an impl is attached to a declaration when their
//! **head constructors** match, and the impl's bounds are *rendered*, not
//! proven — exactly rustdoc's model for `impl<T: Ord> …`.
//!
//! Heads are companion-normalized so the structural container/primitive types
//! and their builtin companion classes agree: `Ty::Int` and `class baml.Int`
//! share the head `baml.Int`; `Ty::List(…)` and `class baml.Array` share
//! `baml.Array`.

use baml_base::{MediaKind, Name};
use baml_type::{PrimitiveType, QualifiedTypeName, Ty};

/// The head constructor of a type, for lossy impl attachment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TyHead {
    /// A nominal head: class, enum, interface, or unexpanded alias —
    /// companion-normalized (`Ty::String` → `baml.String`, `Ty::List` →
    /// `baml.Array`, `Ty::Map` → `baml.Map`, media → `baml.media.*`).
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
pub fn ty_head(ty: &Ty) -> Option<TyHead> {
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
pub fn impl_attaches(impl_head: &TyHead, decl_head: &TyHead) -> bool {
    impl_head == decl_head || *impl_head == TyHead::Blanket
}
