//! A small cache of resolved field slots for `LoadField`/`StoreField`.
//!
//! Resolving field `idx` of an instance walks from the instance to its class,
//! the class's layout and the layout's field table: six dependent loads, where
//! the enum VM indexed a `Vec` in about three. Instance descriptors are
//! interned per layout, so the slot of field `idx` depends only on the
//! instance's descriptor; this cache maps `(descriptor, idx)` to the slot,
//! direct-mapped, so a hit costs the header load and one compare. The
//! bytecode is shared between threads and immutable, so the cache lives on
//! the VM rather than at the instruction.

use bex_vm_types::{
    layout::{DescriptorId, SlotKind},
    types::FieldSlot,
};

/// Entries; a power of two. Small, so a spawned VM stays small.
const ENTRIES: usize = 64;

pub(crate) struct FieldSlotCache {
    /// `(key, slot)`; key 0 is empty (instance descriptors are at least 64).
    entries: Box<[(u64, FieldSlot); ENTRIES]>,
}

impl FieldSlotCache {
    pub(crate) fn new() -> Self {
        let empty = FieldSlot {
            offset: 0,
            kind: SlotKind::Ptr,
        };
        Self {
            entries: Box::new([(0, empty); ENTRIES]),
        }
    }

    #[inline]
    fn key(descriptor: DescriptorId, idx: usize) -> u64 {
        (u64::from(descriptor.0) << 32) | idx as u64
    }

    #[inline]
    fn index(descriptor: DescriptorId, idx: usize) -> usize {
        (descriptor.0 as usize).wrapping_mul(31).wrapping_add(idx) & (ENTRIES - 1)
    }

    /// The cached slot of field `idx` of instances with `descriptor`.
    #[inline]
    pub(crate) fn get(&self, descriptor: DescriptorId, idx: usize) -> Option<FieldSlot> {
        let (key, slot) = self.entries[Self::index(descriptor, idx)];
        (key == Self::key(descriptor, idx)).then_some(slot)
    }

    pub(crate) fn insert(&mut self, descriptor: DescriptorId, idx: usize, slot: FieldSlot) {
        self.entries[Self::index(descriptor, idx)] = (Self::key(descriptor, idx), slot);
    }
}
