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
//! Reads of one element or the length take no lock: they Acquire-load the
//! backing pointer and the length and bounds-check against both. Every
//! mutation, and every whole-list snapshot, takes the list's lock, so no
//! update is lost and a snapshot is never torn. Growth allocates a new
//! backing, copies the elements, and publishes it with a Release store; a
//! reader holding the old backing keeps reading valid (if stale) memory,
//! since a collection cannot run while it reads.

#![allow(unsafe_code)]

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

    /// Hold the list for a mutation or a consistent read. A thread-local
    /// access takes no lock; debug builds check that nobody else holds it.
    pub fn lock(self, access: Access) -> ListGuard {
        let guard = match access {
            Access::Shared => Some(self.mutex().enter()),
            Access::ThreadLocal if cfg!(debug_assertions) => {
                Some(self.mutex().try_enter().unwrap_or_else(|| {
                    panic!("a list accessed as thread-local is locked by another holder")
                }))
            }
            Access::ThreadLocal => None,
        };
        ListGuard {
            list: self,
            access,
            _guard: guard,
        }
    }

    /// Every element, read under the lock.
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

/// The list's lock, held: element reads, whole-list reads and mutations.
pub struct ListGuard {
    list: ListRef,
    access: Access,
    _guard: Option<AccessGuard<'static>>,
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

    #[inline]
    fn slot(&self) -> SlotKind {
        self.list.elem_slot()
    }

    fn set_len(&self, len: usize) {
        self.list
            .word(LIST_LEN_OFFSET)
            .store(len as u64, Ordering::Release);
    }

    /// Element `index`, or `None` past the end.
    #[inline]
    pub fn get(&self, index: usize) -> Option<SlotValue> {
        self.list.get_with(index, self.access)
    }

    /// Every element.
    pub fn to_vec(&self) -> Vec<SlotValue> {
        let Some(backing) = self.backing() else {
            return Vec::new();
        };
        (0..self.len())
            // SAFETY: below the length, so inside the backing.
            .map(|index| unsafe { read_slot(backing.elem(index), self.slot(), self.access) })
            .collect()
    }

    /// Store `value` to element `index`, which must be in bounds.
    ///
    /// # Panics
    /// Past the end.
    pub fn set(&self, index: usize, value: SlotValue) -> Result<(), SlotMismatch> {
        assert!(
            index < self.len(),
            "set element {index} of a {}-element list",
            self.len()
        );
        let backing = self.backing().expect("a non-empty list has a backing");
        // SAFETY: in bounds of the live backing.
        unsafe { write_value(backing.elem(index), self.slot(), value, self.access) }
    }

    /// Make room for `additional` more elements, growing the backing.
    pub fn reserve(&self, alloc: &mut dyn BackingAlloc, additional: usize) {
        let needed = self.len() + additional;
        let cap = self.backing().map_or(0, Backing::cap);
        if needed <= cap {
            return;
        }
        let new_cap = needed.max(cap * 2).max(4);
        self.regrow(alloc, new_cap);
    }

    /// Move the elements to a fresh backing of `cap` and publish it.
    fn regrow(&self, alloc: &mut dyn BackingAlloc, cap: usize) {
        let slot = self.slot();
        let fresh = Backing {
            ptr: alloc.alloc_backing(slot.elem_kind(), cap),
            stride: slot.elem_kind().stride(),
        };
        if let Some(old) = self.backing() {
            for index in 0..self.len().min(cap) {
                // SAFETY: below the length and the new capacity.
                unsafe { copy_slot(old.elem(index), fresh.elem(index), slot, self.access) };
            }
        }
        self.list
            .word(LIST_BACKING_OFFSET)
            .store(Value::object(fresh.ptr).bits(), Ordering::Release);
    }

    /// Append `value`; returns the new length.
    pub fn push(
        &self,
        alloc: &mut dyn BackingAlloc,
        value: SlotValue,
    ) -> Result<usize, SlotMismatch> {
        self.reserve(alloc, 1);
        let len = self.len();
        let backing = self.backing().expect("reserved");
        // SAFETY: below the capacity just reserved.
        unsafe { write_value(backing.elem(len), self.slot(), value, self.access) }?;
        self.set_len(len + 1);
        Ok(len + 1)
    }

    /// Remove and return the last element.
    pub fn pop(&self) -> Option<SlotValue> {
        let len = self.len();
        let last = len.checked_sub(1)?;
        let value = self.get(last)?;
        self.set_len(last);
        self.vacate(last..len);
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
        let len = self.len();
        assert!(index <= len, "insert at {index} of a {len}-element list");
        self.reserve(alloc, 1);
        self.shift(index, index + 1, len - index);
        let backing = self.backing().expect("reserved");
        // SAFETY: below the capacity just reserved.
        unsafe { write_value(backing.elem(index), self.slot(), value, self.access) }?;
        self.set_len(len + 1);
        Ok(())
    }

    /// Remove and return element `index`, shifting the rest down.
    pub fn remove(&self, index: usize) -> Option<SlotValue> {
        let len = self.len();
        let value = self.get(index)?;
        self.shift(index + 1, index, len - index - 1);
        self.set_len(len - 1);
        self.vacate(len - 1..len);
        Some(value)
    }

    /// Keep the first `len` elements.
    pub fn truncate(&self, len: usize) {
        let old = self.len();
        if len >= old {
            return;
        }
        self.set_len(len);
        self.vacate(len..old);
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
        let old = self.len();
        let cap = self.backing().map_or(0, Backing::cap);
        if values.len() > cap {
            self.set_len(0);
            self.regrow(alloc, values.len());
        }
        let backing = self.backing();
        for (index, value) in values.iter().enumerate() {
            let backing = backing.expect("room for every value");
            // SAFETY: below the capacity.
            unsafe { write_value(backing.elem(index), self.slot(), *value, self.access) }?;
        }
        self.set_len(values.len());
        if values.len() < old {
            self.vacate(values.len()..old);
        }
        Ok(())
    }

    /// Move `count` elements from `from` to `to`, overlapping allowed.
    fn shift(&self, from: usize, to: usize, count: usize) {
        let Some(backing) = self.backing() else {
            return;
        };
        let slot = self.slot();
        let mut copy = |index: usize| {
            // SAFETY: both indexes are below the capacity.
            unsafe {
                copy_slot(
                    backing.elem(from + index),
                    backing.elem(to + index),
                    slot,
                    self.access,
                )
            };
        };
        if to > from {
            (0..count).rev().for_each(&mut copy);
        } else {
            (0..count).for_each(&mut copy);
        }
    }

    /// Clear the slots in `range`, past the length, so the collector does not
    /// keep what they held.
    fn vacate(&self, range: std::ops::Range<usize>) {
        let Some(backing) = self.backing() else {
            return;
        };
        let slot = self.slot();
        for index in range {
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
