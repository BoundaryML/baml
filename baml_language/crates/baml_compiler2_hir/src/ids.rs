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
pub struct InterfaceMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeAliasMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TemplateStringMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RetryPolicyMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LetMarker;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImplMarker;

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

/// Hash an `implements` block's structural identity — its interface-target head
/// and for-target head — to 16 bits for use in `LocalItemId<ImplMarker>`.
///
/// Impls have no declared name, so this position-independent seed (plus the
/// `LocalItemId` collision index) gives them stable IDs that survive reordering
/// of unrelated items and whitespace edits. Multiple impls sharing a
/// `(iface_head, for_head)` (e.g. `Converter<int>` and `Converter<float>` on
/// one class) collide here and are disambiguated by the collision index.
pub fn hash_impl_key(iface_head: &baml_base::Name, for_head: &baml_base::Name) -> u16 {
    use std::collections::hash_map::DefaultHasher;

    let mut hasher = DefaultHasher::new();
    iface_head.hash(&mut hasher);
    for_head.hash(&mut hasher);
    #[expect(clippy::cast_possible_truncation)]
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
    Interface,
    TypeAlias,
    Client,
    TemplateString,
    RetryPolicy,
    Let,
    Impl,
}
