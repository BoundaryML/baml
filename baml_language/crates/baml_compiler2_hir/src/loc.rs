//! Interned location structs for compiler2_hir.
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
    ClassMarker, ClientMarker, EnumMarker, FunctionMarker, GeneratorMarker, LocalItemId,
    RetryPolicyMarker, TemplateStringMarker, TestMarker, TypeAliasMarker,
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
pub struct TestLoc<'db> {
    pub file: SourceFile,
    pub id: LocalItemId<TestMarker>,
}

#[salsa::interned]
pub struct GeneratorLoc<'db> {
    pub file: SourceFile,
    pub id: LocalItemId<GeneratorMarker>,
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

impl std::fmt::Debug for TestLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TestLoc(..)")
    }
}

impl std::fmt::Debug for GeneratorLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GeneratorLoc(..)")
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

// ── ItemId ───────────────────────────────────────────────────────────────────

/// Sum type for any top-level item location.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemId<'db> {
    Function(FunctionLoc<'db>),
    Class(ClassLoc<'db>),
    Enum(EnumLoc<'db>),
    TypeAlias(TypeAliasLoc<'db>),
    Client(ClientLoc<'db>),
    Test(TestLoc<'db>),
    Generator(GeneratorLoc<'db>),
    TemplateString(TemplateStringLoc<'db>),
    RetryPolicy(RetryPolicyLoc<'db>),
}

impl std::fmt::Debug for ItemId<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ItemId::Function(_) => write!(f, "ItemId::Function(..)"),
            ItemId::Class(_) => write!(f, "ItemId::Class(..)"),
            ItemId::Enum(_) => write!(f, "ItemId::Enum(..)"),
            ItemId::TypeAlias(_) => write!(f, "ItemId::TypeAlias(..)"),
            ItemId::Client(_) => write!(f, "ItemId::Client(..)"),
            ItemId::Test(_) => write!(f, "ItemId::Test(..)"),
            ItemId::Generator(_) => write!(f, "ItemId::Generator(..)"),
            ItemId::TemplateString(_) => write!(f, "ItemId::TemplateString(..)"),
            ItemId::RetryPolicy(_) => write!(f, "ItemId::RetryPolicy(..)"),
        }
    }
}
