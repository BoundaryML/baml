//! Position-independent item identifiers for `compiler2_hir`.
//!
//! `LocalItemId<T>` packs a 16-bit name hash and a 16-bit collision index
//! into 32 bits, following the same scheme as `baml_compiler_hir::ids`.
//! This is a clean copy — no shared dependency on the old crate.

use std::{
    hash::{Hash, Hasher},
    marker::PhantomData,
};

// ── Marker types — one per item kind ────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClassMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeAliasMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TestMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GeneratorMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TemplateStringMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RetryPolicyMarker;

// ── LocalItemId ──────────────────────────────────────────────────────────────

/// Position-independent item ID.
///
/// Upper 16 bits = name hash, lower 16 bits = collision index.
/// Following rust-analyzer's approach: hash for position-independence,
/// index for collision handling.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalItemId<T> {
    /// Upper 16 bits: hash, lower 16 bits: collision index.
    packed: u32,
    _phantom: PhantomData<T>,
}

impl<T> std::fmt::Debug for LocalItemId<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LocalItemId({:#010x})", self.packed)
    }
}

impl<T> LocalItemId<T> {
    pub const fn new(hash: u16, index: u16) -> Self {
        Self {
            packed: ((hash as u32) << 16) | (index as u32),
            _phantom: PhantomData,
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    pub const fn name_hash(self) -> u16 {
        (self.packed >> 16) as u16
    }

    #[allow(clippy::cast_possible_truncation)]
    pub const fn collision_index(self) -> u16 {
        self.packed as u16
    }

    pub const fn as_u32(self) -> u32 {
        self.packed
    }
}

// ── hash_name ────────────────────────────────────────────────────────────────

/// Hash a `baml_base::Name` to 16 bits for use in `LocalItemId`.
pub fn hash_name(name: &baml_base::Name) -> u16 {
    use std::collections::hash_map::DefaultHasher;

    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    #[allow(clippy::cast_possible_truncation)]
    let h = hasher.finish() as u16;
    h
}

// ── ItemKind ─────────────────────────────────────────────────────────────────

/// Item kinds for collision tracking in the `ItemTree`.
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
