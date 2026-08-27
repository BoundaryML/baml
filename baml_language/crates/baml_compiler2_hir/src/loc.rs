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
    LocalItemId, RetryPolicyMarker, TemplateStringMarker, TestMarker, TypeAliasMarker,
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
pub struct TestLoc<'db> {
    pub file: SourceFile,
    pub id: LocalItemId<TestMarker>,
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

// ── Provenance ───────────────────────────────────────────────────────────────

/// A reference to a possibly-external declaration, algebraic over PROVENANCE.
///
/// Every entity kind that can arrive from outside the compiling database's
/// sources (functions, classes, enums, interfaces, type aliases, lets) is
/// referenced through this ONE shape rather than through partial
/// `*Loc`-keyed maps whose live-only domain a reader cannot see. The variant
/// determines which questions are even answerable:
///
/// - [`DeclRef::Live`] — source available; type-checked here, bytecode
///   emitted here. Everything is askable.
/// - [`DeclRef::Spliced`] — source available (so type-checked here: every
///   salsa query answers), but the compiled artifact comes from a cache —
///   the precompiled-stdlib splice or a Stage-6 clean-file reuse.
/// - [`DeclRef::External`] — no source: the declaration is known only
///   through a mounted surface (a package-interface blob or an engine
///   mount). Only its exported shape is askable; it has no slot, no pooled
///   object, no body. `E` is a kind-specific interned extern identity
///   (surface + path), minted ONCE at the mount boundary — downstream code
///   compares ids and reads rows through memoized queries, never by
///   re-resolving name bundles.
///
/// `L` is the kind's `*Loc` type (salsa's macros take no generics, so the
/// nine loc structs stay concrete and this enum is generic over them); `E`
/// is the kind's extern-loc type, defined where the mounted rows live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclRef<L, E> {
    /// Source available: type-checked and compiled by this database.
    Live(L),
    /// Source available and type-checked here; compiled artifact from cache.
    Spliced(L),
    /// Source unavailable: shape known only through a mounted surface.
    External(E),
}

impl<L, E> DeclRef<L, E> {
    /// The source-backed declaration, when there is one (`Live`/`Spliced`).
    /// `None` IS the answer for an external — not a lookup failure.
    pub fn source_loc(self) -> Option<L> {
        match self {
            DeclRef::Live(loc) | DeclRef::Spliced(loc) => Some(loc),
            DeclRef::External(_) => None,
        }
    }
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

impl std::fmt::Debug for TestLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TestLoc(..)")
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
    Test(TestLoc<'db>),
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
            ItemId::Test(_) => write!(f, "ItemId::Test(..)"),
            ItemId::TemplateString(_) => write!(f, "ItemId::TemplateString(..)"),
            ItemId::RetryPolicy(_) => write!(f, "ItemId::RetryPolicy(..)"),
            ItemId::Let(_) => write!(f, "ItemId::Let(..)"),
        }
    }
}
