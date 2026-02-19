//! HIR item identifiers with Salsa interning.
//!
//! This module defines stable IDs for all top-level items in BAML.
//! IDs are interned via Salsa, providing:
//! - Stability across edits (content-based, not order-based)
//! - Compactness (u32 instead of full location data)
//! - Efficient comparison and hashing

use std::marker::PhantomData;

/// Identifier for a class definition.
pub use crate::loc::ClassLoc as ClassId;
/// Identifier for a client configuration.
pub use crate::loc::ClientLoc as ClientId;
/// Identifier for an enum definition.
pub use crate::loc::EnumLoc as EnumId;
// Note: In Salsa 2022, interned types are their own IDs.
// The #[salsa::interned] macro in loc.rs creates these types directly.
// We re-export them here as type aliases for clarity.
/// Identifier for a function (LLM or expression).
/// This is the interned `FunctionLoc` from loc.rs.
pub use crate::loc::FunctionLoc as FunctionId;
/// Identifier for a generator configuration.
pub use crate::loc::GeneratorLoc as GeneratorId;
/// Identifier for a retry policy.
pub use crate::loc::RetryPolicyLoc as RetryPolicyId;
/// Identifier for a template string definition.
pub use crate::loc::TemplateStringLoc as TemplateStringId;
/// Identifier for a test definition.
pub use crate::loc::TestLoc as TestId;
/// Identifier for a type alias.
pub use crate::loc::TypeAliasLoc as TypeAliasId;

// Manual Debug implementations for Salsa interned types
// These types don't auto-derive Debug, so we provide simple implementations

impl std::fmt::Debug for FunctionId<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FunctionId(..)")
    }
}

impl std::fmt::Debug for ClassId<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ClassId(..)")
    }
}

impl std::fmt::Debug for EnumId<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EnumId(..)")
    }
}

impl std::fmt::Debug for TypeAliasId<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TypeAliasId(..)")
    }
}

impl std::fmt::Debug for ClientId<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ClientId(..)")
    }
}

impl std::fmt::Debug for TestId<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TestId(..)")
    }
}

impl std::fmt::Debug for GeneratorId<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GeneratorId(..)")
    }
}

impl std::fmt::Debug for TemplateStringId<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TemplateStringId(..)")
    }
}

impl std::fmt::Debug for RetryPolicyId<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RetryPolicyId(..)")
    }
}

/// Union type for any top-level item.
///
/// Note: Salsa interned types have a `'db` lifetime, so `ItemId` must also have one.
#[derive(Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub enum ItemId<'db> {
    Function(FunctionId<'db>),
    Class(ClassId<'db>),
    Enum(EnumId<'db>),
    TypeAlias(TypeAliasId<'db>),
    Client(ClientId<'db>),
    Generator(GeneratorId<'db>),
    Test(TestId<'db>),
    TemplateString(TemplateStringId<'db>),
    RetryPolicy(RetryPolicyId<'db>),
}

// Manual Debug impl since Salsa interned types don't auto-derive Debug
impl std::fmt::Debug for ItemId<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ItemId::Function(_) => write!(f, "Function(_)"),
            ItemId::Class(_) => write!(f, "Class(_)"),
            ItemId::Enum(_) => write!(f, "Enum(_)"),
            ItemId::TypeAlias(_) => write!(f, "TypeAlias(_)"),
            ItemId::Client(_) => write!(f, "Client(_)"),
            ItemId::Generator(_) => write!(f, "Generator(_)"),
            ItemId::Test(_) => write!(f, "Test(_)"),
            ItemId::TemplateString(_) => write!(f, "TemplateString(_)"),
            ItemId::RetryPolicy(_) => write!(f, "RetryPolicy(_)"),
        }
    }
}

/// Local ID within an `ItemTree` (type-safe, collision-resistant).
///
/// Packs a 16-bit hash and 16-bit collision index into 32 bits.
/// This follows rust-analyzer's approach: hash for position-independence,
/// index for collision handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalItemId<T> {
    /// Upper 16 bits: hash, Lower 16 bits: collision index
    packed: u32,
    _phantom: PhantomData<T>,
}

impl<T> LocalItemId<T> {
    /// Create a new `LocalItemId` from hash and collision index.
    pub const fn new(hash: u16, index: u16) -> Self {
        let packed = ((hash as u32) << 16) | (index as u32);
        LocalItemId {
            packed,
            _phantom: PhantomData,
        }
    }

    /// Extract the hash portion (upper 16 bits).
    #[allow(clippy::cast_possible_truncation)]
    pub const fn hash(self) -> u16 {
        (self.packed >> 16) as u16
    }

    /// Extract the collision index (lower 16 bits).
    #[allow(clippy::cast_possible_truncation)]
    pub const fn index(self) -> u16 {
        self.packed as u16
    }

    pub const fn as_u32(self) -> u32 {
        self.packed
    }
}

/// Hash a name to 16 bits for use in `LocalItemId`.
pub fn hash_name(name: &baml_base::Name) -> u16 {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };

    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    #[allow(clippy::cast_possible_truncation)]
    let hash = hasher.finish() as u16;
    hash
}

/// Item kinds for collision tracking.
/// Used as part of the composite key `(ItemKind, hash)` in the collision map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Function,
    Class,
    Enum,
    TypeAlias,
    Client,
    Generator,
    Test,
    TemplateString,
    RetryPolicy,
}
