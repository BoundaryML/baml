//! Lists in the shared object format: a fixed list object pointing at a
//! backing object that holds the elements inline.
//!
//! Replaces the old `Array` (`Object::Array`: a boxed element type and a
//! `LockedContainer<Vec<Value>>` of tagged values).
//!
//! ```text
//! #[repr(C)]
//! struct List {
//!     hdr: Header,             // descriptor = LIST_DESCS[element slot kind]
//!     len: AtomicU64,          // elements in use
//!     backing: AtomicU64,      // pointer slot: the Backing, or null while empty
//!     elem_ty: AtomicU64,      // pointer slot: an `Object::Type` holding the element type
//!     lock: LazyBiasedMutex,   // one byte; taken by every mutation
//!     _pad: [u8; 7],
//! }
//!
//! #[repr(C)]
//! struct Backing {
//!     hdr: Header,             // descriptor = BACKING_DESCS[element kind]
//!     cap: u64,
//!     elems: [slot; cap],      // Raw8, Ptr or Tagged16, as the list's descriptor says
//! }
//! ```
//!
//! Lists are lock-free (design section 11, decision 1): a push stores the
//! element (Relaxed for a raw slot, Release for a pointer) then the length
//! with Release; a reader Acquire-loads the length and the backing and bounds
//! every index by the capacity of the backing it loaded. Growth allocates a
//! new backing, copies, and publishes it with a Release store; a reader still
//! holding the old one reads valid (if stale) memory, since a collection
//! cannot run while it reads. A racing push can be lost; no interleaving
//! reads or writes past a backing. `insert`, `remove` and `replace_all` move
//! many elements and serialize among themselves on the list's lock. The
//! `locked-containers` feature makes every mutation hold the lock, as the old
//! `LockedContainer` did.

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
    layout::{DescriptorId, ElemKind, RawKind, SlotKind},
    lazy_biased_mutex::{AccessGuard, LazyBiasedMutex},
    object_format::{
        Access, Format, HEADER_BYTES, SlotMismatch, SlotValue, copy_slot, read_slot,
        write_float_slot, write_slot,
    },
};

/// Byte offsets, from the header, of a list's fields.
pub const LIST_LEN_OFFSET: u32 = 8;
pub const LIST_BACKING_OFFSET: u32 = 16;
pub const LIST_ELEM_TY_OFFSET: u32 = 24;
pub const LIST_LOCK_OFFSET: u32 = 32;
/// Bytes of a list object, header included.
pub const LIST_BYTES: u32 = 40;

/// Byte offsets, from the header, of a backing's capacity and first element.
pub const BACKING_CAP_OFFSET: u32 = 8;
pub const BACKING_ELEMS_OFFSET: u32 = 16;

/// The list descriptor for each element slot kind. Fixed by the object
/// format, so compiled code and the interpreter agree: a `List<int>` is
/// always `LIST_DESCS[0]`.
pub const LIST_DESCS: [(SlotKind, DescriptorId); 5] = [
    (SlotKind::Raw(RawKind::Int), DescriptorId(1)),
    (SlotKind::Raw(RawKind::Float), DescriptorId(2)),
    (SlotKind::Raw(RawKind::Bool), DescriptorId(3)),
    (SlotKind::Ptr, DescriptorId(4)),
    (SlotKind::Tagged16, DescriptorId(5)),
];

/// The backing descriptor for each element kind.
pub const BACKING_DESCS: [(ElemKind, DescriptorId); 3] = [
    (ElemKind::Raw8, DescriptorId(6)),
    (ElemKind::Ptr, DescriptorId(7)),
    (ElemKind::Tagged16, DescriptorId(8)),
];

/// The descriptor of lists whose elements are stored in `slot`.
pub fn list_descriptor(slot: SlotKind) -> DescriptorId {
    LIST_DESCS
        .iter()
        .find(|(kind, _)| *kind == slot)
        .map(|(_, id)| *id)
        .expect("every slot kind has a list descriptor")
}

/// The descriptor of backings whose elements are `elem`.
pub fn backing_descriptor(elem: ElemKind) -> DescriptorId {
    BACKING_DESCS
        .iter()
        .find(|(kind, _)| *kind == elem)
        .map(|(_, id)| *id)
        .expect("every element kind has a backing descriptor")
}

/// The element slot of the list descriptor `id`, if it is one.
pub fn list_slot_of(id: DescriptorId) -> Option<SlotKind> {
    LIST_DESCS
        .iter()
        .find(|(_, desc)| *desc == id)
        .map(|(kind, _)| *kind)
}

/// The element kind of the backing descriptor `id`, if it is one.
pub fn backing_elem_of(id: DescriptorId) -> Option<ElemKind> {
    BACKING_DESCS
        .iter()
        .find(|(_, desc)| *desc == id)
        .map(|(kind, _)| *kind)
}

/// Allocates a list's backing: a zeroed cell of `backing_descriptor(elem)`
/// with room for `cap` elements, its capacity written. Implemented by the
/// heap's allocator, which this crate cannot name.
pub trait BackingAlloc {
    fn alloc_backing(&mut self, elem: ElemKind, cap: usize) -> HeapPtr;
}

/// A list on the heap.
///
/// Valid while the list is: like [`HeapPtr::get`], it hands out nothing that
/// outlives the object, and the caller keeps it alive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ListRef(HeapPtr);

impl ListRef {
    /// The list `ptr` names, if it is one.
    ///
    /// # Safety
    /// `ptr` names a live object.
    #[inline]
    pub unsafe fn of(ptr: HeapPtr) -> Option<Self> {
        // SAFETY: forwarded to the caller.
        matches!(unsafe { ptr.format() }, Format::List(_)).then_some(Self(ptr))
    }

    /// # Safety
    /// `ptr` names a live list.
    #[inline]
    pub unsafe fn new_unchecked(ptr: HeapPtr) -> Self {
        debug_assert!(matches!(
            // SAFETY: forwarded to the caller.
            unsafe { ptr.format() },
            Format::List(_)
        ));
        Self(ptr)
    }

    /// Fill a fresh, zeroed list cell: its element type object.
    ///
    /// # Safety
    /// `ptr` names a freshly allocated list cell; `elem_ty` is an `Object::Type`.
    pub unsafe fn init(ptr: HeapPtr, elem_ty: HeapPtr) -> Self {
        let list = Self(ptr);
        list.word(LIST_ELEM_TY_OFFSET)
            .store(Value::object(elem_ty).bits(), Ordering::Relaxed);
        list
    }

    #[inline]
    pub fn ptr(self) -> HeapPtr {
        self.0
    }

    #[inline]
    fn word(self, offset: u32) -> &'static AtomicU64 {
        // SAFETY: the offset is inside the live list; the header precedes the
        // pointer.
        unsafe {
            &*self
                .0
                .as_ptr()
                .cast::<u8>()
                .sub(HEADER_BYTES)
                .add(offset as usize)
                .cast::<AtomicU64>()
        }
    }

    #[inline]
    fn mutex(self) -> &'static LazyBiasedMutex {
        // SAFETY: the lock byte is inside the live list, zero (unlocked) when
        // allocated.
        unsafe {
            &*self
                .0
                .as_ptr()
                .cast::<u8>()
                .sub(HEADER_BYTES)
                .add(LIST_LOCK_OFFSET as usize)
                .cast::<LazyBiasedMutex>()
        }
    }

    /// The slot kind every element is stored in.
    #[inline]
    pub fn elem_slot(self) -> SlotKind {
        // SAFETY: a live list has a header.
        let id = unsafe { self.0.header() }.descriptor;
        list_slot_of(id).expect("a list's descriptor is a list descriptor")
    }

    /// The declared element type.
    pub fn element_ty(self) -> &'static RealizedTy {
        let bits = self.word(LIST_ELEM_TY_OFFSET).load(Ordering::Relaxed);
        // SAFETY: the element type slot is set at allocation to a type object
        // the list keeps alive.
        let ptr = unsafe { Value::from_bits(bits) }
            .as_object_ptr()
            .expect("a list's element type is set");
        // SAFETY: as above.
        match unsafe { ptr.get() } {
            Object::Type(value) => &value.ty,
            other => unreachable!("a list's element type slot holds {other}"),
        }
    }

    /// The number of elements.
    #[inline]
    pub fn len(self) -> usize {
        self.word(LIST_LEN_OFFSET).load(Ordering::Acquire) as usize
    }

    #[inline]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    #[inline]
    fn backing(self, order: Ordering) -> Option<Backing> {
        let bits = self.word(LIST_BACKING_OFFSET).load(order);
        // SAFETY: the backing slot holds zero or a live backing's pointer.
        let ptr = unsafe { Value::from_bits(bits) }.as_object_ptr()?;
        Some(Backing {
            ptr,
            stride: self.elem_slot().elem_kind().stride(),
        })
    }

    /// The backing object and its capacity, `None` while the list has none.
    pub fn backing_object(self) -> Option<(HeapPtr, usize)> {
        let backing = self.backing(Ordering::Acquire)?;
        Some((backing.ptr, backing.cap()))
    }

    /// Element `index`, without the lock, or `None` past the end.
    #[inline]
    pub fn get(self, index: usize) -> Option<SlotValue> {
        self.get_with(index, Access::Shared)
    }

    /// [`Self::get`] with `access`.
    #[inline]
    pub fn get_with(self, index: usize, access: Access) -> Option<SlotValue> {
        let backing = self.backing(Ordering::Acquire)?;
        if index >= self.len() || index >= backing.cap() {
            return None;
        }
        // SAFETY: in bounds of the live backing.
        Some(unsafe { read_slot(backing.elem(index), self.elem_slot(), access) })
    }

    /// The list for mutation. Single-element operations take no lock (see
    /// the module doc); with the `locked-containers` feature every operation
    /// through the guard holds the list's lock, as `LockedContainer` did.
    pub fn lock(self, access: Access) -> ListGuard {
        let guard = (cfg!(feature = "locked-containers") && access == Access::Shared)
            .then(|| self.mutex().enter());
        ListGuard {
            list: self,
            access,
            held: guard,
        }
    }

    /// Every element.
    pub fn snapshot(self, access: Access) -> Vec<SlotValue> {
        self.lock(access).to_vec()
    }
}

/// A list's backing object.
#[derive(Clone, Copy)]
struct Backing {
    ptr: HeapPtr,
    stride: usize,
}

impl Backing {
    fn base(self) -> *const u8 {
        // SAFETY: the header precedes the pointer.
        unsafe { self.ptr.as_ptr().cast::<u8>().sub(HEADER_BYTES) }
    }

    fn cap(self) -> usize {
        // SAFETY: the capacity word is inside the live backing.
        let word = unsafe {
            &*self
                .base()
                .add(BACKING_CAP_OFFSET as usize)
                .cast::<AtomicU64>()
        };
        word.load(Ordering::Relaxed) as usize
    }

    fn elem(self, index: usize) -> *const u8 {
        // SAFETY: callers keep `index` below the capacity.
        unsafe {
            self.base()
                .add(BACKING_ELEMS_OFFSET as usize + index * self.stride)
        }
    }
}

/// Fill a fresh, zeroed backing cell: its capacity, and null in every
/// tagged element, since a zeroed tagged slot reads as held (tag 0). Called by
/// [`BackingAlloc`] implementations.
///
/// # Safety
/// `ptr` names a freshly allocated backing cell of room for `cap` `elem`s.
pub unsafe fn init_backing(ptr: HeapPtr, cap: usize, elem: ElemKind) {
    let backing = Backing {
        ptr,
        stride: elem.stride(),
    };
    // SAFETY: forwarded to the caller.
    let word = unsafe {
        &*backing
            .base()
            .add(BACKING_CAP_OFFSET as usize)
            .cast::<AtomicU64>()
    };
    word.store(cap as u64, Ordering::Relaxed);
    if elem != ElemKind::Tagged16 {
        return;
    }
    for index in 0..cap {
        // SAFETY: below the capacity of the fresh backing.
        unsafe {
            write_slot(
                backing.elem(index),
                SlotKind::Tagged16,
                Value::NULL,
                Access::ThreadLocal,
            )
        }
        .expect("null fits a tagged slot");
    }
}

/// Bytes of a backing cell for `cap` elements of `elem`.
pub fn backing_bytes(elem: ElemKind, cap: usize) -> usize {
    BACKING_ELEMS_OFFSET as usize + cap * elem.stride()
}

/// A list being read or mutated (see [`ListRef::lock`]).
///
/// Every access bounds its index by the capacity of the backing it loaded,
/// so a racing grow, push or pop can lose an update but never reach past a
/// backing. `push`, `pop`, `set`, `truncate` and `clear` take no lock;
/// `insert`, `remove` and `replace_all` move many elements and take the
/// list's lock among themselves, short and only for their own duration.
pub struct ListGuard {
    list: ListRef,
    access: Access,
    held: Option<AccessGuard<'static>>,
}

impl ListGuard {
    #[inline]
    pub fn list(&self) -> ListRef {
        self.list
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.list.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    fn backing(&self) -> Option<Backing> {
        self.list.backing(Ordering::Acquire)
    }

    /// The length, then the backing (both Acquire, so the backing is at least
    /// as new as the length's writer saw), and the elements usable in it.
    #[inline]
    fn view(&self) -> (Option<Backing>, usize) {
        let len = self.len();
        let backing = self.backing();
        let usable = backing.map_or(0, |backing| len.min(backing.cap()));
        (backing, usable)
    }

    #[inline]
    fn slot(&self) -> SlotKind {
        self.list.elem_slot()
    }

    fn set_len(&self, len: usize) {
        self.list
            .word(LIST_LEN_OFFSET)
            .store(len as u64, Ordering::Release);
    }

    /// The list's lock for a many-element mutation, unless already held or
    /// the list is thread-local.
    fn exclusive(&self) -> Option<AccessGuard<'static>> {
        (self.held.is_none() && self.access == Access::Shared).then(|| self.list.mutex().enter())
    }

    /// Element `index`, or `None` past the end.
    #[inline]
    pub fn get(&self, index: usize) -> Option<SlotValue> {
        self.list.get_with(index, self.access)
    }

    /// Every element.
    pub fn to_vec(&self) -> Vec<SlotValue> {
        let (Some(backing), usable) = self.view() else {
            return Vec::new();
        };
        (0..usable)
            // SAFETY: below the loaded backing's capacity.
            .map(|index| unsafe { read_slot(backing.elem(index), self.slot(), self.access) })
            .collect()
    }

    /// Store `value` to element `index`. Past the end (a racing pop or grow
    /// moved it) the store is dropped.
    pub fn set(&self, index: usize, value: SlotValue) -> Result<(), SlotMismatch> {
        let (Some(backing), usable) = self.view() else {
            return Ok(());
        };
        if index >= usable {
            return Ok(());
        }
        // SAFETY: below the loaded backing's capacity.
        unsafe { write_value(backing.elem(index), self.slot(), value, self.access) }
    }

    /// A backing with room for `needed` elements: the current one, or a
    /// larger one with the elements copied in, published.
    fn room_for(&self, alloc: &mut dyn BackingAlloc, needed: usize) -> Backing {
        let backing = self.backing();
        let cap = backing.map_or(0, Backing::cap);
        match backing {
            Some(backing) if needed <= cap => backing,
            _ => self.regrow(alloc, needed.max(cap * 2).max(4)),
        }
    }

    /// Make room for `additional` more elements, growing the backing.
    pub fn reserve(&self, alloc: &mut dyn BackingAlloc, additional: usize) {
        self.room_for(alloc, self.len() + additional);
    }

    /// Copy the elements to a fresh backing of `cap` and publish it with a
    /// Release store. A reader still holding the old backing reads it until
    /// it next loads the pointer; the collector keeps it alive meanwhile.
    fn regrow(&self, alloc: &mut dyn BackingAlloc, cap: usize) -> Backing {
        let slot = self.slot();
        let fresh = Backing {
            ptr: alloc.alloc_backing(slot.elem_kind(), cap),
            stride: slot.elem_kind().stride(),
        };
        if let (Some(old), usable) = self.view() {
            for index in 0..usable.min(cap) {
                // SAFETY: below both backings' capacities.
                unsafe { copy_slot(old.elem(index), fresh.elem(index), slot, self.access) };
            }
        }
        self.list
            .word(LIST_BACKING_OFFSET)
            .store(Value::object(fresh.ptr).bits(), Ordering::Release);
        fresh
    }

    /// Append `value` (element store, then a Release length store); returns
    /// the new length. A racing push may be lost.
    pub fn push(
        &self,
        alloc: &mut dyn BackingAlloc,
        value: SlotValue,
    ) -> Result<usize, SlotMismatch> {
        let len = self.len();
        let backing = self.room_for(alloc, len + 1);
        // SAFETY: below the capacity of the backing just made room in.
        unsafe { write_value(backing.elem(len), self.slot(), value, self.access) }?;
        self.set_len(len + 1);
        Ok(len + 1)
    }

    /// Remove and return the last element.
    pub fn pop(&self) -> Option<SlotValue> {
        let (_, usable) = self.view();
        let last = usable.checked_sub(1)?;
        let value = self.get(last)?;
        self.set_len(last);
        self.vacate(last..last + 1);
        Some(value)
    }

    /// Insert `value` at `index`, shifting the rest up.
    ///
    /// # Panics
    /// If `index` is past the length.
    pub fn insert(
        &self,
        alloc: &mut dyn BackingAlloc,
        index: usize,
        value: SlotValue,
    ) -> Result<(), SlotMismatch> {
        let _exclusive = self.exclusive();
        let len = self.len();
        assert!(index <= len, "insert at {index} of a {len}-element list");
        let backing = self.room_for(alloc, len + 1);
        self.shift(backing, index, index + 1, len - index);
        // SAFETY: below the capacity of the backing just made room in.
        unsafe { write_value(backing.elem(index), self.slot(), value, self.access) }?;
        self.set_len(len + 1);
        Ok(())
    }

    /// Remove and return element `index`, shifting the rest down.
    pub fn remove(&self, index: usize) -> Option<SlotValue> {
        let _exclusive = self.exclusive();
        let (backing, usable) = self.view();
        if index >= usable {
            return None;
        }
        let value = self.get(index)?;
        let backing = backing?;
        self.shift(backing, index + 1, index, usable - index - 1);
        self.set_len(usable - 1);
        self.vacate(usable - 1..usable);
        Some(value)
    }

    /// Keep the first `len` elements.
    pub fn truncate(&self, len: usize) {
        let (_, usable) = self.view();
        if len >= usable {
            return;
        }
        self.set_len(len);
        self.vacate(len..usable);
    }

    pub fn clear(&self) {
        self.truncate(0);
    }

    /// Replace every element with `values`.
    pub fn replace_all(
        &self,
        alloc: &mut dyn BackingAlloc,
        values: &[SlotValue],
    ) -> Result<(), SlotMismatch> {
        let _exclusive = self.exclusive();
        if values.is_empty() {
            self.truncate(0);
            return Ok(());
        }
        let (_, old) = self.view();
        let backing = match self.backing() {
            Some(backing) if values.len() <= backing.cap() => backing,
            _ => {
                self.set_len(0);
                self.regrow(alloc, values.len().max(1))
            }
        };
        for (index, value) in values.iter().enumerate() {
            // SAFETY: below the capacity checked or just allocated.
            unsafe { write_value(backing.elem(index), self.slot(), *value, self.access) }?;
        }
        self.set_len(values.len());
        if values.len() < old {
            self.vacate(values.len()..old);
        }
        Ok(())
    }

    /// Move `count` elements from `from` to `to` within `backing`, overlapping
    /// allowed, clamped to its capacity.
    fn shift(&self, backing: Backing, from: usize, to: usize, count: usize) {
        let slot = self.slot();
        let cap = backing.cap();
        let count = count.min(cap.saturating_sub(from.max(to)));
        let mut copy = |index: usize| {
            // SAFETY: both indexes are below the capacity.
            unsafe {
                copy_slot(
                    backing.elem(from + index),
                    backing.elem(to + index),
                    slot,
                    self.access,
                );
            }
        };
        if to > from {
            (0..count).rev().for_each(&mut copy);
        } else {
            (0..count).for_each(&mut copy);
        }
    }

    /// Clear the slots in `range` of the current backing, clamped to its
    /// capacity, so the collector does not keep what they held.
    fn vacate(&self, range: std::ops::Range<usize>) {
        let Some(backing) = self.backing() else {
            return;
        };
        let slot = self.slot();
        for index in range.start.min(backing.cap())..range.end.min(backing.cap()) {
            // SAFETY: below the capacity.
            unsafe { write_slot(backing.elem(index), slot, Value::NULL, self.access) }
                .expect("null fits every slot");
        }
    }
}

/// Write a slot value, keeping a float unboxed.
///
/// # Safety
/// As for [`write_slot`].
unsafe fn write_value(
    addr: *const u8,
    slot: SlotKind,
    value: SlotValue,
    access: Access,
) -> Result<(), SlotMismatch> {
    match value {
        // SAFETY: forwarded to the caller.
        SlotValue::Value(value) => unsafe { write_slot(addr, slot, value, access) },
        // SAFETY: forwarded to the caller.
        SlotValue::Float(float) => unsafe { write_float_slot(addr, slot, float, access) },
    }
}

impl std::fmt::Display for ListRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<list len={}>", self.len())
    }
}
