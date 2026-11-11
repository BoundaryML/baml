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
//! Reads and value updates take no lock: every index is bounded by the
//! backing it reads, so a racing mutation can lose an update but never reach
//! past a backing. Insertions, removals and clears serialize on the map's
//! lock, and each bumps the epoch, so a multi-step lookup still detects a
//! membership change between its steps. The `locked-containers` feature makes
//! every operation hold the lock, as the old container did.

#![allow(unsafe_code)]
#![allow(
    clippy::cast_ptr_alignment,
    clippy::cast_possible_truncation,
    reason = "slot and field addresses are header-relative offsets the layout keeps 8-byte \
              (tagged: 16-byte) aligned, and capacities and offsets fit their words"
)]

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

    fn mutex(self) -> &'static LazyBiasedMutex {
        // SAFETY: the lock byte is inside the live map, zero (unlocked) when
        // allocated.
        unsafe { &*self.field(MAP_LOCK_OFFSET).cast::<LazyBiasedMutex>() }
    }

    /// The map for reading and mutation. Reads and value updates take no
    /// lock; insertions, removals and clears serialize on the map's lock.
    /// With the `locked-containers` feature every operation through the
    /// guard holds the lock, as `LockedContainer` did.
    pub fn lock(self, access: Access) -> MapGuard {
        // SAFETY: the lock byte is inside the live map, zero (unlocked) when
        // allocated.
        let guard = (cfg!(feature = "locked-containers") && access == Access::Shared)
            .then(|| self.mutex().enter());
        MapGuard {
            map: self,
            access,
            held: guard,
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

/// The three backings of one generation. A reader may load backings of
/// different generations while a grow races it, so every index is bounded by
/// the backing it reads: keys and values by their own capacity, hashes and
/// probe slots by the index's, whose word 0 records its entry capacity.
#[derive(Clone, Copy)]
struct Columns {
    keys: Column,
    values: Column,
    index: Column,
}

impl Columns {
    /// Bytes of the three backings.
    fn bytes(self) -> usize {
        [self.keys, self.values, self.index]
            .into_iter()
            .map(|column| BACKING_ELEMS_OFFSET as usize + column.cap() * column.stride)
            .sum()
    }

    /// Entries the index has hash words for.
    fn cap(self) -> usize {
        self.index.word(0).load(Ordering::Relaxed) as usize
    }

    /// Probe slots in the index, after the capacity word and the hashes.
    fn table_len(self) -> usize {
        self.index.cap() - 1 - self.cap()
    }

    /// Whether entry `pos` lies in every backing.
    fn holds(self, pos: usize) -> bool {
        pos < self.keys.cap() && pos < self.values.cap() && pos < self.cap()
    }

    fn hash(self, pos: usize) -> u64 {
        self.index.word(1 + pos).load(Ordering::Relaxed)
    }

    fn set_hash(self, pos: usize, hash: u64) {
        self.index.word(1 + pos).store(hash, Ordering::Relaxed);
    }

    fn probe(self, slot: usize) -> &'static AtomicU64 {
        self.index.word(1 + self.cap() + slot)
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
    held: Option<AccessGuard<'static>>,
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
        Some(Columns {
            keys: self.column(MAP_KEYS_OFFSET, key.elem_kind())?,
            values: self.column(MAP_VALUES_OFFSET, value.elem_kind())?,
            index: self.column(MAP_INDEX_OFFSET, ElemKind::Raw8)?,
        })
    }

    /// The map's lock for an insertion, removal or clear, unless already held
    /// or the map is thread-local.
    fn exclusive(&self) -> Option<AccessGuard<'static>> {
        (self.held.is_none() && self.access == Access::Shared).then(|| self.map.mutex().enter())
    }

    /// The backings when entry `pos` is in the map and in every backing; a
    /// racing removal or grow can take it away.
    fn entry_columns(&self, pos: usize) -> Option<Columns> {
        let len = self.len();
        let columns = self.columns()?;
        (pos < len && columns.holds(pos)).then_some(columns)
    }

    /// The key of entry `pos`; null when a race took it away.
    pub fn key(&self, pos: usize) -> SlotValue {
        let Some(columns) = self.entry_columns(pos) else {
            return SlotValue::Value(Value::NULL);
        };
        // SAFETY: inside the keys backing.
        unsafe { read_slot(columns.keys.at(pos), self.map.slots().0, self.access) }
    }

    /// The value of entry `pos`; null when a race took it away.
    pub fn value(&self, pos: usize) -> SlotValue {
        let Some(columns) = self.entry_columns(pos) else {
            return SlotValue::Value(Value::NULL);
        };
        // SAFETY: inside the values backing.
        unsafe { read_slot(columns.values.at(pos), self.map.slots().1, self.access) }
    }

    /// The stored hash of entry `pos`.
    pub fn hash(&self, pos: usize) -> u64 {
        self.entry_columns(pos)
            .map_or(0, |columns| columns.hash(pos))
    }

    /// Every entry, in order.
    pub fn entries(&self) -> Vec<(SlotValue, SlotValue)> {
        (0..self.len())
            .filter(|&pos| self.entry_columns(pos).is_some())
            .map(|pos| (self.key(pos), self.value(pos)))
            .collect()
    }

    /// The positions of the entries whose key has hash `hash`, in insertion
    /// order.
    pub fn candidates(&self, hash: u64) -> Vec<usize> {
        let Some(columns) = self.columns() else {
            return Vec::new();
        };
        let len = self.len().min(columns.cap());
        let table = columns.table_len();
        let mask = table - 1;
        #[expect(clippy::cast_possible_truncation, reason = "masked to the table")]
        let start = (hash as usize) & mask;
        let mut found: Vec<usize> = (0..table)
            .map(|step| columns.probe((start + step) & mask).load(Ordering::Relaxed) as usize)
            .take_while(|&entry| entry != 0)
            .map(|entry| entry - 1)
            .filter(|&pos| pos < len && columns.hash(pos) == hash)
            .collect();
        found.sort_unstable();
        found
    }

    /// The position of the entry with hash `hash` whose key `matches`.
    pub fn find(&self, hash: u64, mut matches: impl FnMut(SlotValue) -> bool) -> Option<usize> {
        self.candidates(hash)
            .into_iter()
            .find(|&pos| matches(self.key(pos)))
    }

    /// Replace entry `pos`'s value, keeping its position. Takes no lock; a
    /// racing removal can lose the update.
    pub fn set_value(&self, pos: usize, value: SlotValue) -> Result<(), SlotMismatch> {
        let Some(columns) = self.entry_columns(pos) else {
            return Ok(());
        };
        // SAFETY: inside the values backing.
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
        let _exclusive = self.exclusive();
        let pos = self.len();
        let columns = self.reserve(alloc, pos + 1);
        let (key_slot, value_slot) = self.map.slots();
        // SAFETY: below the capacity just reserved.
        unsafe {
            write_entry(columns.keys.at(pos), key_slot, key, self.access)?;
            write_entry(columns.values.at(pos), value_slot, value, self.access)?;
        }
        columns.set_hash(pos, hash);
        place(columns, pos, hash);
        self.set_len(pos + 1);
        self.advance_epoch();
        Ok(pos)
    }

    /// Remove entry `pos`, shifting the later entries down; returns its value.
    pub fn remove(&self, pos: usize) -> SlotValue {
        let _exclusive = self.exclusive();
        let value = self.value(pos);
        let Some(columns) = self.entry_columns(pos) else {
            return value;
        };
        let len = self
            .len()
            .min(columns.cap())
            .min(columns.keys.cap())
            .min(columns.values.cap());
        let (key_slot, value_slot) = self.map.slots();
        for from in pos + 1..len {
            // SAFETY: both positions are below every backing's capacity.
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
            columns.set_hash(from - 1, columns.hash(from));
        }
        self.set_len(len - 1);
        self.vacate(columns, len - 1..len);
        rebuild_table(columns, len - 1);
        self.advance_epoch();
        value
    }

    /// Remove every entry.
    pub fn clear(&self) {
        let _exclusive = self.exclusive();
        let len = self.len();
        let Some(columns) = self.columns() else {
            return;
        };
        if len == 0 {
            return;
        }
        self.set_len(0);
        self.vacate(columns, 0..len);
        rebuild_table(columns, 0);
        self.advance_epoch();
    }

    /// Make room for `needed` entries, moving to larger backings.
    fn reserve(&self, alloc: &mut dyn BackingAlloc, needed: usize) -> Columns {
        let columns = self.columns();
        if let Some(columns) = columns
            && needed <= columns.cap()
            && needed <= columns.keys.cap()
            && needed <= columns.values.cap()
        {
            return columns;
        }
        let cap = columns.map_or(0, Columns::cap);
        self.regrow(alloc, needed.max(cap * 2).max(4))
    }

    /// Copy the entries to fresh backings of `cap` and publish them, each
    /// with a Release store.
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
                ptr: alloc.alloc_backing(ElemKind::Raw8, 1 + cap + table_len_for(cap)),
                stride: ElemKind::Raw8.stride(),
            },
        };
        fresh.index.word(0).store(cap as u64, Ordering::Relaxed);
        let mut len = 0;
        let old = self.columns();
        if let Some(old) = old {
            len = self
                .len()
                .min(old.cap())
                .min(old.keys.cap())
                .min(old.values.cap())
                .min(cap);
            for pos in 0..len {
                // SAFETY: below every capacity, old and new.
                unsafe {
                    copy_slot(old.keys.at(pos), fresh.keys.at(pos), key_slot, self.access);
                    copy_slot(
                        old.values.at(pos),
                        fresh.values.at(pos),
                        value_slot,
                        self.access,
                    );
                }
                fresh.set_hash(pos, old.hash(pos));
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
        if let Some(old) = old {
            alloc.retire_backing(old.bytes());
        }
        fresh
    }

    /// Clear the key and value slots in `range`, clamped to the backings, so
    /// the collector does not keep what they held.
    fn vacate(&self, columns: Columns, range: std::ops::Range<usize>) {
        let (key_slot, value_slot) = self.map.slots();
        let end = range.end.min(columns.keys.cap()).min(columns.values.cap());
        for pos in range.start.min(end)..end {
            // SAFETY: below both capacities.
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
