//! Interned location structs for `compiler2_hir`.
//!
//! Each `*Loc` uniquely identifies where an item is defined:
//!   `SourceFile` (Salsa input) + `LocalItemId<Marker>`.
//!
//! Nine `#[salsa::interned]` structs — one per item kind.
//! Modeled after `baml_compiler_hir::loc` but independent types.
//!
//! Manual `Debug` impls are required because Salsa-generated interned types
//! don't auto-derive `Debug` (their repr is opaque).

use baml_base::SourceFile;

use crate::ids::{
    ClassMarker, ClientMarker, EnumMarker, FunctionMarker, ImplMarker, InterfaceMarker, LetMarker,
    LocalItemId, RetryPolicyMarker, TemplateStringMarker, TypeAliasMarker,
};

#[salsa::interned]
pub struct FunctionLoc<'db> {
    pub file: SourceFile,
    pub id: LocalItemId<FunctionMarker>,
}

#[salsa::interned]
pub struct ClassLoc<'db> {
    pub file: SourceFile,
    pub id: LocalItemId<ClassMarker>,
}

#[salsa::interned]
pub struct EnumLoc<'db> {
    pub file: SourceFile,
    pub id: LocalItemId<EnumMarker>,
}

#[salsa::interned]
pub struct InterfaceLoc<'db> {
    pub file: SourceFile,
    pub id: LocalItemId<InterfaceMarker>,
}

#[salsa::interned]
pub struct TypeAliasLoc<'db> {
    pub file: SourceFile,
    pub id: LocalItemId<TypeAliasMarker>,
}

#[salsa::interned]
pub struct ClientLoc<'db> {
    pub file: SourceFile,
    pub id: LocalItemId<ClientMarker>,
}

#[salsa::interned]
pub struct TemplateStringLoc<'db> {
    pub file: SourceFile,
    pub id: LocalItemId<TemplateStringMarker>,
}

#[salsa::interned]
pub struct RetryPolicyLoc<'db> {
    pub file: SourceFile,
    pub id: LocalItemId<RetryPolicyMarker>,
}

#[salsa::interned]
pub struct LetLoc<'db> {
    pub file: SourceFile,
    pub id: LocalItemId<LetMarker>,
}

/// Stable identity for an `implements` block (both kinds: in-body and
/// out-of-body). Unlike the other `*Loc` types, an impl has no declared name,
/// so its `LocalItemId` is seeded from the impl's structural identity (its
/// interface-target and for-target heads) rather than a name hash — see
/// `ItemTree` allocation. Impls are not name-addressable, so `ImplLoc` is
/// intentionally absent from `Definition`/`ItemId`.
#[salsa::interned]
pub struct ImplLoc<'db> {
    pub file: SourceFile,
    pub id: LocalItemId<ImplMarker>,
}

// ── Manual Debug impls ───────────────────────────────────────────────────────
// Salsa interned types don't auto-derive Debug. These minimal impls satisfy
// the Debug bound when *Loc types appear inside derived Debug types.

impl std::fmt::Debug for FunctionLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FunctionLoc(..)")
    }
}

impl std::fmt::Debug for ClassLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ClassLoc(..)")
    }
}

impl std::fmt::Debug for EnumLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EnumLoc(..)")
    }
}

impl std::fmt::Debug for InterfaceLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "InterfaceLoc(..)")
    }
}

impl std::fmt::Debug for TypeAliasLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TypeAliasLoc(..)")
    }
}

impl std::fmt::Debug for ClientLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ClientLoc(..)")
    }
}

impl std::fmt::Debug for TemplateStringLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TemplateStringLoc(..)")
    }
}

impl std::fmt::Debug for RetryPolicyLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RetryPolicyLoc(..)")
    }
}

impl std::fmt::Debug for LetLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LetLoc(..)")
    }
}

impl std::fmt::Debug for ImplLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ImplLoc(..)")
    }
}

// ── ItemId ───────────────────────────────────────────────────────────────────

/// Sum type for any top-level item location.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemId<'db> {
    Function(FunctionLoc<'db>),
    Class(ClassLoc<'db>),
    Enum(EnumLoc<'db>),
    Interface(InterfaceLoc<'db>),
    TypeAlias(TypeAliasLoc<'db>),
    Client(ClientLoc<'db>),
    TemplateString(TemplateStringLoc<'db>),
    RetryPolicy(RetryPolicyLoc<'db>),
    Let(LetLoc<'db>),
}

impl std::fmt::Debug for ItemId<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ItemId::Function(_) => write!(f, "ItemId::Function(..)"),
            ItemId::Class(_) => write!(f, "ItemId::Class(..)"),
            ItemId::Enum(_) => write!(f, "ItemId::Enum(..)"),
            ItemId::Interface(_) => write!(f, "ItemId::Interface(..)"),
            ItemId::TypeAlias(_) => write!(f, "ItemId::TypeAlias(..)"),
            ItemId::Client(_) => write!(f, "ItemId::Client(..)"),
            ItemId::TemplateString(_) => write!(f, "ItemId::TemplateString(..)"),
            ItemId::RetryPolicy(_) => write!(f, "ItemId::RetryPolicy(..)"),
            ItemId::Let(_) => write!(f, "ItemId::Let(..)"),
        }
    }
}
