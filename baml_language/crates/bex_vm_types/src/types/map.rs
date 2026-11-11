//! Maps in the shared object format: a fixed map object pointing at three
//! backing objects (keys, values, and an index of hashes and probe slots).
//!
//! Replaces the old `Map` (`Object::Map`: two boxed types and a
//! `LockedContainer<Box<MapData>>` holding an `IndexMap` of tagged entries and
//! a `HashMap` of per-hash buckets).
//!
//! ```text
//! #[repr(C)]
//! struct Map {
//!     hdr: Header,             // descriptor = map_descriptor(key slot, value slot)
//!     len: AtomicU64,          // entries in use
//!     keys: AtomicU64,         // pointer slot: Backing of keys, or null while empty
//!     values: AtomicU64,       // pointer slot: Backing of values
//!     index: AtomicU64,        // pointer slot: Raw8 Backing, [hash; cap] then [probe; tcap]
//!     ty: AtomicU64,           // pointer slot: an `Object::Type` holding `map<K, V>`
//!     epoch: AtomicU64,        // bumped by every membership change
//!     lock: LazyBiasedMutex,   // one byte; taken by every access
//!     _pad: [u8; 7],
//! }
//! ```
//!
//! Entries keep insertion order: entry `i` is `keys[i]`, `values[i]`,
//! `hash[i]`. The probe table (linear probing, a power of two at least twice
//! the entry capacity) holds `position + 1`, zero for empty. A removal shifts
//! the later entries down and rebuilds the table, so there are no tombstones;
//! an entry's position is its ID until the epoch changes. Keys are hashed by
//! the caller (the interpreter's hashing, which may run user code), so the
//! map stores hashes, never computes them.
//!
//! Every access takes the map's lock, as the old container did, so a
//! multi-step lookup can validate its snapshot by epoch.

#![allow(unsafe_code)]

use std::sync::atomic::{AtomicU64, Ordering};

use crate::{
    HeapPtr, Object, RealizedTy, Value,
    layout::{DescriptorId, ElemKind, SlotKind},
    lazy_biased_mutex::{AccessGuard, LazyBiasedMutex},
    object_format::{
        Access, Format, HEADER_BYTES, SlotMismatch, SlotValue, copy_slot, read_slot, write_slot,
    },
    types::{BACKING_ELEMS_OFFSET, BackingAlloc, LIST_DESCS},
};

/// Byte offsets, from the header, of a map's fields.
pub const MAP_LEN_OFFSET: u32 = 8;
pub const MAP_KEYS_OFFSET: u32 = 16;
pub const MAP_VALUES_OFFSET: u32 = 24;
pub const MAP_INDEX_OFFSET: u32 = 32;
pub const MAP_TY_OFFSET: u32 = 40;
pub const MAP_EPOCH_OFFSET: u32 = 48;
pub const MAP_LOCK_OFFSET: u32 = 56;
/// Bytes of a map object, header included.
pub const MAP_BYTES: u32 = 64;

/// The first map descriptor; the rest follow, five per key slot kind.
const MAP_DESC_BASE: u32 = 9;

/// The position of `slot` in the fixed slot-kind order ([`LIST_DESCS`]).
fn slot_index(slot: SlotKind) -> u32 {
    let index = LIST_DESCS
        .iter()
        .position(|(kind, _)| *kind == slot)
        .expect("every slot kind is listed");
    u32::try_from(index).expect("five slot kinds")
}

/// The descriptor of maps whose keys are stored in `key` and values in
/// `value`. Fixed by the object format: `map<string, int>` is always
/// `map_descriptor(Ptr, Raw(Int))`.
pub fn map_descriptor(key: SlotKind, value: SlotKind) -> DescriptorId {
    let kinds = u32::try_from(LIST_DESCS.len()).expect("five slot kinds");
    DescriptorId(MAP_DESC_BASE + slot_index(key) * kinds + slot_index(value))
}

/// Every map descriptor with its key and value slots.
pub fn map_descriptors() -> impl Iterator<Item = (SlotKind, SlotKind, DescriptorId)> {
    LIST_DESCS.iter().flat_map(|(key, _)| {
        LIST_DESCS
            .iter()
            .map(move |(value, _)| (*key, *value, map_descriptor(*key, *value)))
    })
}

/// The key and value slots of the map descriptor `id`, if it is one.
pub fn map_slots_of(id: DescriptorId) -> Option<(SlotKind, SlotKind)> {
    let kinds = u32::try_from(LIST_DESCS.len()).expect("five slot kinds");
    let offset = id.0.checked_sub(MAP_DESC_BASE)?;
    let (key, value) = (offset / kinds, offset % kinds);
    let slot = |index: u32| LIST_DESCS.get(index as usize).map(|(kind, _)| *kind);
    Some((slot(key)?, slot(value)?))
}

/// An entry's position in its map, valid while the map's epoch is unchanged.
pub type EntryId = u64;

/// Hash a native string key with the same framing as the VM's `Hash` driver.
pub fn map_string_hash(key: &str) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write(&[5]);
    hasher.write(&(key.len() as u64).to_le_bytes());
    hasher.write(key.as_bytes());
    hasher.finish()
}

/// An entry read out of a map, its value boxed as a local.
#[derive(Clone, Copy, Debug)]
pub struct MapEntry {
    pub key: Value,
    pub value: Value,
    pub hash: u64,
}

/// A lookup's snapshot is stale: the map's membership changed since.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapEpochChanged;

/// A map on the heap. Valid while the map is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MapRef(HeapPtr);

impl MapRef {
    /// The map `ptr` names, if it is one.
    ///
    /// # Safety
    /// `ptr` names a live object.
    #[inline]
    pub unsafe fn of(ptr: HeapPtr) -> Option<Self> {
        // SAFETY: forwarded to the caller.
        matches!(unsafe { ptr.format() }, Format::Map(..)).then_some(Self(ptr))
    }

    /// # Safety
    /// `ptr` names a live map.
    #[inline]
    pub unsafe fn new_unchecked(ptr: HeapPtr) -> Self {
        Self(ptr)
    }

    /// Fill a fresh, zeroed map cell: its type object.
    ///
    /// # Safety
    /// `ptr` names a freshly allocated map cell; `ty` is an `Object::Type`
    /// holding a map type.
    pub unsafe fn init(ptr: HeapPtr, ty: HeapPtr) -> Self {
        let map = Self(ptr);
        map.word(MAP_TY_OFFSET)
            .store(Value::object(ty).bits(), Ordering::Relaxed);
        map
    }

    #[inline]
    pub fn ptr(self) -> HeapPtr {
        self.0
    }

    #[inline]
    fn field(self, offset: u32) -> *const u8 {
        // SAFETY: the offset is inside the live map.
        unsafe {
            self.0
                .as_ptr()
                .cast::<u8>()
                .sub(HEADER_BYTES)
                .add(offset as usize)
        }
    }

    #[inline]
    fn word(self, offset: u32) -> &'static AtomicU64 {
        // SAFETY: a word-aligned field of the live map.
        unsafe { &*self.field(offset).cast::<AtomicU64>() }
    }

    /// The key and value slot kinds.
    #[inline]
    pub fn slots(self) -> (SlotKind, SlotKind) {
        // SAFETY: a live map has a header.
        let id = unsafe { self.0.header() }.descriptor;
        map_slots_of(id).expect("a map's descriptor is a map descriptor")
    }

    fn map_ty(self) -> (&'static RealizedTy, &'static RealizedTy) {
        let bits = self.word(MAP_TY_OFFSET).load(Ordering::Relaxed);
        // SAFETY: the type slot is set at allocation to a type object the map
        // keeps alive.
        let ptr = unsafe { Value::from_bits(bits) }
            .as_object_ptr()
            .expect("a map's type is set");
        // SAFETY: as above.
        match unsafe { ptr.get() } {
            Object::Type(value) => match &value.ty {
                RealizedTy::Map { key, value } => (key, value),
                other => unreachable!("a map's type slot holds {other:?}"),
            },
            other => unreachable!("a map's type slot holds {other}"),
        }
    }

    /// The declared key type.
    pub fn key_ty(self) -> &'static RealizedTy {
        self.map_ty().0
    }

    /// The declared value type.
    pub fn value_ty(self) -> &'static RealizedTy {
        self.map_ty().1
    }

    /// The number of entries, without the lock.
    #[inline]
    pub fn len(self) -> usize {
        self.word(MAP_LEN_OFFSET).load(Ordering::Acquire) as usize
    }

    #[inline]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Every entry, read under the lock.
    pub fn snapshot_entries(self) -> Vec<(SlotValue, SlotValue)> {
        self.lock(Access::Shared).entries()
    }

    /// The key, value and index backings, `None` while the map has none.
    pub fn backing_objects(self) -> Option<[HeapPtr; 3]> {
        let guard = self.lock(Access::Shared);
        let columns = guard.columns()?;
        Some([columns.keys.ptr, columns.values.ptr, columns.index.ptr])
    }

    /// Hold the map. A thread-local access takes no lock; debug builds check
    /// that nobody else holds it.
    pub fn lock(self, access: Access) -> MapGuard {
        // SAFETY: the lock byte is inside the live map, zero (unlocked) when
        // allocated.
        let mutex = unsafe { &*self.field(MAP_LOCK_OFFSET).cast::<LazyBiasedMutex>() };
        let guard = match access {
            Access::Shared => Some(mutex.enter()),
            Access::ThreadLocal if cfg!(debug_assertions) => {
                Some(mutex.try_enter().unwrap_or_else(|| {
                    panic!("a map accessed as thread-local is locked by another holder")
                }))
            }
            Access::ThreadLocal => None,
        };
        MapGuard {
            map: self,
            access,
            _guard: guard,
        }
    }
}

/// One backing of a map: keys, values or the index.
#[derive(Clone, Copy)]
struct Column {
    ptr: HeapPtr,
    stride: usize,
}

impl Column {
    fn base(self) -> *const u8 {
        // SAFETY: the header precedes the pointer.
        unsafe { self.ptr.as_ptr().cast::<u8>().sub(HEADER_BYTES) }
    }

    fn cap(self) -> usize {
        // SAFETY: the capacity word follows the header.
        let word = unsafe { &*self.base().add(8).cast::<AtomicU64>() };
        word.load(Ordering::Relaxed) as usize
    }

    fn at(self, index: usize) -> *const u8 {
        // SAFETY: callers keep `index` below the capacity.
        unsafe {
            self.base()
                .add(BACKING_ELEMS_OFFSET as usize + index * self.stride)
        }
    }

    fn word(self, index: usize) -> &'static AtomicU64 {
        // SAFETY: a Raw8 element below the capacity.
        unsafe { &*self.at(index).cast::<AtomicU64>() }
    }
}

/// The three backings, present together once the map has room for an entry.
#[derive(Clone, Copy)]
struct Columns {
    keys: Column,
    values: Column,
    index: Column,
    /// Entry capacity: the keys' and values' capacity, and the number of
    /// hash words at the start of the index.
    cap: usize,
}

impl Columns {
    /// Probe slots in the index, after the hashes.
    fn table_len(self) -> usize {
        self.index.cap() - self.cap
    }

    fn hash(self, pos: usize) -> u64 {
        self.index.word(pos).load(Ordering::Relaxed)
    }

    fn probe(self, slot: usize) -> &'static AtomicU64 {
        self.index.word(self.cap + slot)
    }
}

/// The probe table size for `cap` entries: a power of two at least twice it.
fn table_len_for(cap: usize) -> usize {
    (cap * 2).next_power_of_two().max(8)
}

/// The map's lock, held.
pub struct MapGuard {
    map: MapRef,
    access: Access,
    _guard: Option<AccessGuard<'static>>,
}

impl MapGuard {
    #[inline]
    pub fn map(&self) -> MapRef {
        self.map
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Bumped by every insertion, removal and clear; a lookup that let go of
    /// the lock checks it before using a position it found.
    pub fn epoch(&self) -> u64 {
        self.map.word(MAP_EPOCH_OFFSET).load(Ordering::Relaxed)
    }

    fn advance_epoch(&self) {
        let epoch = self.epoch().checked_add(1).expect("map epoch exhausted");
        self.map
            .word(MAP_EPOCH_OFFSET)
            .store(epoch, Ordering::Relaxed);
    }

    fn set_len(&self, len: usize) {
        self.map
            .word(MAP_LEN_OFFSET)
            .store(len as u64, Ordering::Release);
    }

    fn column(&self, offset: u32, elem: ElemKind) -> Option<Column> {
        let bits = self.map.word(offset).load(Ordering::Acquire);
        // SAFETY: the slot holds zero or a live backing's pointer.
        let ptr = unsafe { Value::from_bits(bits) }.as_object_ptr()?;
        Some(Column {
            ptr,
            stride: elem.stride(),
        })
    }

    fn columns(&self) -> Option<Columns> {
        let (key, value) = self.map.slots();
        let keys = self.column(MAP_KEYS_OFFSET, key.elem_kind())?;
        Some(Columns {
            keys,
            values: self.column(MAP_VALUES_OFFSET, value.elem_kind())?,
            index: self.column(MAP_INDEX_OFFSET, ElemKind::Raw8)?,
            cap: keys.cap(),
        })
    }

    /// The key of entry `pos`.
    ///
    /// # Panics
    /// Past the last entry.
    pub fn key(&self, pos: usize) -> SlotValue {
        let columns = self.entry_columns(pos);
        // SAFETY: below the length, so inside the backing.
        unsafe { read_slot(columns.keys.at(pos), self.map.slots().0, self.access) }
    }

    /// The value of entry `pos`.
    ///
    /// # Panics
    /// Past the last entry.
    pub fn value(&self, pos: usize) -> SlotValue {
        let columns = self.entry_columns(pos);
        // SAFETY: below the length, so inside the backing.
        unsafe { read_slot(columns.values.at(pos), self.map.slots().1, self.access) }
    }

    /// The stored hash of entry `pos`.
    pub fn hash(&self, pos: usize) -> u64 {
        self.entry_columns(pos).hash(pos)
    }

    fn entry_columns(&self, pos: usize) -> Columns {
        assert!(
            pos < self.len(),
            "entry {pos} of a {}-entry map",
            self.len()
        );
        self.columns().expect("a non-empty map has backings")
    }

    /// Every entry, in order.
    pub fn entries(&self) -> Vec<(SlotValue, SlotValue)> {
        (0..self.len())
            .map(|pos| (self.key(pos), self.value(pos)))
            .collect()
    }

    /// The positions of the entries whose key has hash `hash`, in insertion
    /// order.
    pub fn candidates(&self, hash: u64) -> Vec<usize> {
        let Some(columns) = self.columns() else {
            return Vec::new();
        };
        let mask = columns.table_len() - 1;
        let mut found = Vec::new();
        #[expect(clippy::cast_possible_truncation, reason = "masked to the table")]
        let mut slot = (hash as usize) & mask;
        loop {
            let entry = columns.probe(slot).load(Ordering::Relaxed) as usize;
            if entry == 0 {
                break;
            }
            if columns.hash(entry - 1) == hash {
                found.push(entry - 1);
            }
            slot = (slot + 1) & mask;
        }
        found.sort_unstable();
        found
    }

    /// The position of the entry with hash `hash` whose key `matches`.
    pub fn find(&self, hash: u64, mut matches: impl FnMut(SlotValue) -> bool) -> Option<usize> {
        self.candidates(hash)
            .into_iter()
            .find(|&pos| matches(self.key(pos)))
    }

    /// Replace entry `pos`'s value, keeping its position.
    pub fn set_value(&self, pos: usize, value: SlotValue) -> Result<(), SlotMismatch> {
        let columns = self.entry_columns(pos);
        // SAFETY: below the length, so inside the backing.
        unsafe {
            write_entry(
                columns.values.at(pos),
                self.map.slots().1,
                value,
                self.access,
            )
        }
    }

    /// Append an entry whose key the caller knows is not in the map; returns
    /// its position.
    pub fn insert(
        &self,
        alloc: &mut dyn BackingAlloc,
        hash: u64,
        key: SlotValue,
        value: SlotValue,
    ) -> Result<usize, SlotMismatch> {
        let pos = self.len();
        let columns = self.reserve(alloc, pos + 1);
        let (key_slot, value_slot) = self.map.slots();
        // SAFETY: below the capacity just reserved.
        unsafe {
            write_entry(columns.keys.at(pos), key_slot, key, self.access)?;
            write_entry(columns.values.at(pos), value_slot, value, self.access)?;
        }
        columns.index.word(pos).store(hash, Ordering::Relaxed);
        place(columns, pos, hash);
        self.set_len(pos + 1);
        self.advance_epoch();
        Ok(pos)
    }

    /// Remove entry `pos`, shifting the later entries down; returns its value.
    pub fn remove(&self, pos: usize) -> SlotValue {
        let value = self.value(pos);
        let columns = self.entry_columns(pos);
        let len = self.len();
        let (key_slot, value_slot) = self.map.slots();
        for from in pos + 1..len {
            // SAFETY: both positions are below the length.
            unsafe {
                copy_slot(
                    columns.keys.at(from),
                    columns.keys.at(from - 1),
                    key_slot,
                    self.access,
                );
                copy_slot(
                    columns.values.at(from),
                    columns.values.at(from - 1),
                    value_slot,
                    self.access,
                );
            }
            let hash = columns.hash(from);
            columns.index.word(from - 1).store(hash, Ordering::Relaxed);
        }
        self.set_len(len - 1);
        self.vacate(columns, len - 1..len);
        rebuild_table(columns, len - 1);
        self.advance_epoch();
        value
    }

    /// Remove every entry.
    pub fn clear(&self) {
        let len = self.len();
        if len == 0 {
            return;
        }
        let columns = self.columns().expect("a non-empty map has backings");
        self.set_len(0);
        self.vacate(columns, 0..len);
        rebuild_table(columns, 0);
        self.advance_epoch();
    }

    /// Make room for `needed` entries, moving to larger backings.
    fn reserve(&self, alloc: &mut dyn BackingAlloc, needed: usize) -> Columns {
        if let Some(columns) = self.columns()
            && needed <= columns.cap
        {
            return columns;
        }
        let cap = self.columns().map_or(0, |columns| columns.cap);
        self.regrow(alloc, needed.max(cap * 2).max(4))
    }

    fn regrow(&self, alloc: &mut dyn BackingAlloc, cap: usize) -> Columns {
        let (key_slot, value_slot) = self.map.slots();
        let fresh = Columns {
            keys: Column {
                ptr: alloc.alloc_backing(key_slot.elem_kind(), cap),
                stride: key_slot.elem_kind().stride(),
            },
            values: Column {
                ptr: alloc.alloc_backing(value_slot.elem_kind(), cap),
                stride: value_slot.elem_kind().stride(),
            },
            index: Column {
                ptr: alloc.alloc_backing(ElemKind::Raw8, cap + table_len_for(cap)),
                stride: ElemKind::Raw8.stride(),
            },
            cap,
        };
        let len = self.len();
        if let Some(old) = self.columns() {
            for pos in 0..len {
                // SAFETY: below the length and both capacities.
                unsafe {
                    copy_slot(old.keys.at(pos), fresh.keys.at(pos), key_slot, self.access);
                    copy_slot(
                        old.values.at(pos),
                        fresh.values.at(pos),
                        value_slot,
                        self.access,
                    );
                }
                fresh
                    .index
                    .word(pos)
                    .store(old.hash(pos), Ordering::Relaxed);
            }
        }
        rebuild_table(fresh, len);
        for (offset, column) in [
            (MAP_KEYS_OFFSET, fresh.keys),
            (MAP_VALUES_OFFSET, fresh.values),
            (MAP_INDEX_OFFSET, fresh.index),
        ] {
            self.map
                .word(offset)
                .store(Value::object(column.ptr).bits(), Ordering::Release);
        }
        fresh
    }

    /// Clear the key and value slots in `range` so the collector does not
    /// keep what they held.
    fn vacate(&self, columns: Columns, range: std::ops::Range<usize>) {
        let (key_slot, value_slot) = self.map.slots();
        for pos in range {
            // SAFETY: below the capacity.
            unsafe {
                write_slot(columns.keys.at(pos), key_slot, Value::NULL, self.access)
                    .expect("null fits every slot");
                write_slot(columns.values.at(pos), value_slot, Value::NULL, self.access)
                    .expect("null fits every slot");
            }
        }
    }
}

/// Put entry `pos` with `hash` in the first empty probe slot.
fn place(columns: Columns, pos: usize, hash: u64) {
    let mask = columns.table_len() - 1;
    #[expect(clippy::cast_possible_truncation, reason = "masked to the table")]
    let mut slot = (hash as usize) & mask;
    while columns.probe(slot).load(Ordering::Relaxed) != 0 {
        slot = (slot + 1) & mask;
    }
    columns.probe(slot).store(pos as u64 + 1, Ordering::Relaxed);
}

/// Rebuild the probe table for the first `len` entries.
fn rebuild_table(columns: Columns, len: usize) {
    for slot in 0..columns.table_len() {
        columns.probe(slot).store(0, Ordering::Relaxed);
    }
    for pos in 0..len {
        place(columns, pos, columns.hash(pos));
    }
}

/// Write an entry half, keeping a float unboxed.
///
/// # Safety
/// As for [`write_slot`].
unsafe fn write_entry(
    addr: *const u8,
    slot: SlotKind,
    value: SlotValue,
    access: Access,
) -> Result<(), SlotMismatch> {
    match value {
        // SAFETY: forwarded to the caller.
        SlotValue::Value(value) => unsafe { write_slot(addr, slot, value, access) },
        // SAFETY: forwarded to the caller.
        SlotValue::Float(float) => unsafe {
            crate::object_format::write_float_slot(addr, slot, float, access)
        },
    }
}

impl std::fmt::Display for MapRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<map len={}>", self.len())
    }
}
