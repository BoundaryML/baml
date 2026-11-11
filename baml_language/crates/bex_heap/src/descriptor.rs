//! Object descriptors: how the collector reads a cell.
//!
//! Every cell starts with a [`Header`] naming its [`Descriptor`] by index in
//! the heap's [`DescriptorTable`]. A descriptor says where the cell's
//! pointers are, either as byte offsets of pointer and tagged slots (the
//! shared object format of the runtime design) or, for an object that is
//! still a Rust value, as hooks into Rust code ([`Trace::Rust`]). Descriptor
//! IDs never change after allocation.
//!
//! The object-kind, slot-kind and element-kind names are PR B's
//! ([`bex_vm_types::layout`]): the builtin adapters and the collector read
//! the same descriptor.
//!
//! The VM's `Object` enum is one such Rust value: every VM allocation is a
//! cell of descriptor [`OBJECT_DESC`], whose hooks call the enum-specific
//! tracing in [`crate::object_trace`].

use std::{borrow::Cow, collections::HashMap, sync::OnceLock};

pub use bex_vm_types::layout::{DescriptorId, ElemKind, Kind, RawKind, SlotKind};
use bex_vm_types::{
    HeapPtr, InstanceLayout, Meter, Object, RealizedTy,
    object_format::FIRST_INSTANCE_DESC,
    types::{
        BACKING_DESCS, BACKING_ELEMS_OFFSET, LIST_BACKING_OFFSET, LIST_BYTES, LIST_DESCS,
        LIST_ELEM_TY_OFFSET, MAP_BYTES, MAP_INDEX_OFFSET, MAP_KEYS_OFFSET, MAP_TY_OFFSET,
        MAP_VALUES_OFFSET, backing_descriptor, list_descriptor, map_descriptor, map_descriptors,
    },
};
use parking_lot::Mutex;

use crate::space::{GRANULE_BYTES, granule_align};
/// Reflection identity of the type a descriptor lays out.
pub type TypeId = u32;
/// Index of the finalizer a descriptor's objects run when they die.
pub type FinalizerId = u32;

pub use bex_vm_types::object_format::{HEADER_BYTES, Header, OBJECT_DESC, TaggedKind, TaggedSlot};
use bex_vm_types::types::{STR_BYTES_OFFSET, STR_DESC};

/// What the heap reads and writes of a [`Header`] beyond the object format.
pub(crate) trait CellHeader {
    /// The header of `cell`.
    ///
    /// # Safety
    /// `cell` is an allocated cell.
    unsafe fn at<'a>(cell: usize) -> &'a Header;

    /// The header of a fresh cell of `cell_bytes`.
    fn for_cell(descriptor: DescriptorId, cell_bytes: usize, large: bool) -> Header;

    /// Size of a small cell. Zero for a large one.
    fn cell_bytes(&self) -> usize;
}

impl CellHeader for Header {
    #[inline]
    unsafe fn at<'a>(cell: usize) -> &'a Header {
        // SAFETY: forwarded to the caller.
        unsafe { &*(cell as *const Header) }
    }

    #[inline]
    fn for_cell(descriptor: DescriptorId, cell_bytes: usize, large: bool) -> Header {
        let granules = if large {
            0
        } else {
            u16::try_from(cell_bytes / GRANULE_BYTES).expect("small cells fit a u16 granule count")
        };
        Header::new(descriptor, granules)
    }

    #[inline]
    fn cell_bytes(&self) -> usize {
        usize::from(self.granules()) * GRANULE_BYTES
    }
}

/// How the collector finds a cell's pointers.
#[derive(Clone, Copy, Debug)]
pub enum Trace {
    /// From the descriptor's slot offsets and element kind.
    Slots,
    /// A Rust value placed after the header. Its pointers are found, its
    /// destructor run and its outside storage measured by Rust code.
    Rust(&'static RustHooks),
}

/// Rust code for a [`Trace::Rust`] object. Each takes a pointer to the value,
/// just after the header.
#[derive(Debug)]
pub struct RustHooks {
    /// Push every heap pointer the value holds.
    pub trace: unsafe fn(*const u8, &mut Vec<HeapPtr>),
    /// Drop the value in place.
    pub drop: unsafe fn(*mut u8),
    /// Report what the value keeps alive outside its cell.
    pub measure: unsafe fn(*mut u8, &mut Meter),
}

/// How the collector reads one kind of cell.
#[derive(Clone, Debug)]
pub struct Descriptor {
    pub kind: Kind,
    pub trace: Trace,
    /// Bytes from the cell start (header included): the whole object for a
    /// fixed object, the part before the first element when `elems` is set.
    pub size: u32,
    /// Elements repeat from `size` to the end of the cell.
    pub elems: Option<ElemKind>,
    /// Byte offsets, from the cell start, of pointer slots.
    pub ptr_slots: Cow<'static, [u32]>,
    /// Byte offsets, from the cell start, of [`TaggedSlot`]s.
    pub tagged_slots: Cow<'static, [u32]>,
    /// The interpreter's view, in declaration order.
    pub slot_kinds: Cow<'static, [SlotKind]>,
    pub type_id: TypeId,
    pub finalizer: Option<FinalizerId>,
}

/// A descriptor the heap refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DescriptorError {
    #[error("size {0} is smaller than the header")]
    SmallerThanHeader(u32),
    #[error("slot at offset {0} overlaps the header, is misaligned, or overruns the object")]
    BadSlot(u32),
    #[error("a Rust-hooked descriptor has slot offsets or elements")]
    RustWithSlots,
}

/// Bytes of a raw or pointer slot, and the alignment of every slot.
const WORD_BYTES: usize = 8;

impl Descriptor {
    /// A descriptor for a Rust value of type `T`, opaque to builtins.
    pub const fn rust<T>(hooks: &'static RustHooks, type_id: TypeId) -> Self {
        Self {
            kind: Kind::Opaque,
            trace: Trace::Rust(hooks),
            size: (HEADER_BYTES + size_of::<T>()) as u32,
            elems: None,
            ptr_slots: Cow::Borrowed(&[]),
            tagged_slots: Cow::Borrowed(&[]),
            slot_kinds: Cow::Borrowed(&[]),
            type_id,
            finalizer: None,
        }
    }

    /// A fixed object of `kind` whose fields have the types `fields`, laid
    /// out after the header in declaration order. Each field's slot is
    /// [`SlotKind::of_type`]'s, the one mapping from types to slots.
    pub fn for_fields(kind: Kind, fields: &[RealizedTy], type_id: TypeId) -> Self {
        let slot_kinds: Vec<SlotKind> = fields.iter().map(SlotKind::of_type).collect();
        let offsets: Vec<usize> = slot_kinds
            .iter()
            .scan(HEADER_BYTES, |next, slot| {
                let offset = *next;
                *next += slot.elem_kind().stride();
                Some(offset)
            })
            .collect();
        let offsets_of = |wanted: ElemKind| -> Vec<u32> {
            slot_kinds
                .iter()
                .zip(&offsets)
                .filter(|(slot, _)| slot.elem_kind() == wanted)
                .map(|(_, &offset)| u32::try_from(offset).expect("object size fits u32"))
                .collect()
        };
        let size = HEADER_BYTES
            + slot_kinds
                .iter()
                .map(|slot| slot.elem_kind().stride())
                .sum::<usize>();
        Self {
            kind,
            trace: Trace::Slots,
            size: u32::try_from(size).expect("object size fits u32"),
            elems: None,
            ptr_slots: Cow::Owned(offsets_of(ElemKind::Ptr)),
            tagged_slots: Cow::Owned(offsets_of(ElemKind::Tagged16)),
            slot_kinds: Cow::Owned(slot_kinds),
            type_id,
            finalizer: None,
        }
    }

    /// The descriptor of lists whose elements are stored in `slot`: the
    /// backing and element type pointers.
    fn list(slot: SlotKind) -> Self {
        Self {
            kind: Kind::List,
            trace: Trace::Slots,
            size: LIST_BYTES,
            elems: None,
            ptr_slots: Cow::Owned(vec![LIST_BACKING_OFFSET, LIST_ELEM_TY_OFFSET]),
            tagged_slots: Cow::Borrowed(&[]),
            slot_kinds: Cow::Owned(vec![slot]),
            type_id: list_descriptor(slot).0,
            finalizer: None,
        }
    }

    /// The descriptor of maps with `key` and `value` slots: the three
    /// backing pointers and the type pointer.
    fn map(key: SlotKind, value: SlotKind) -> Self {
        Self {
            kind: Kind::Map,
            trace: Trace::Slots,
            size: MAP_BYTES,
            elems: None,
            ptr_slots: Cow::Owned(vec![
                MAP_KEYS_OFFSET,
                MAP_VALUES_OFFSET,
                MAP_INDEX_OFFSET,
                MAP_TY_OFFSET,
            ]),
            tagged_slots: Cow::Borrowed(&[]),
            slot_kinds: Cow::Owned(vec![key, value]),
            type_id: map_descriptor(key, value).0,
            finalizer: None,
        }
    }

    /// The descriptor of list backings holding `elem` elements.
    fn backing(elem: ElemKind) -> Self {
        Self {
            kind: Kind::Backing,
            trace: Trace::Slots,
            size: BACKING_ELEMS_OFFSET,
            elems: Some(elem),
            ptr_slots: Cow::Borrowed(&[]),
            tagged_slots: Cow::Borrowed(&[]),
            slot_kinds: Cow::Borrowed(&[]),
            type_id: backing_descriptor(elem).0,
            finalizer: None,
        }
    }

    /// The descriptor of flat strings: the lengths and hash word, then the
    /// bytes to the end of the cell. No pointers.
    fn str() -> Self {
        Self {
            kind: Kind::Str,
            trace: Trace::Slots,
            size: STR_BYTES_OFFSET,
            elems: None,
            ptr_slots: Cow::Borrowed(&[]),
            tagged_slots: Cow::Borrowed(&[]),
            slot_kinds: Cow::Borrowed(&[]),
            type_id: STR_DESC.0,
            finalizer: None,
        }
    }

    /// The descriptor of instances laid out by `layout`: the class and type
    /// pointers and the declared fields' slots.
    pub fn for_instance(layout: &InstanceLayout) -> Self {
        Self {
            kind: Kind::Instance,
            trace: Trace::Slots,
            size: u32::try_from(layout.size()).expect("object size fits u32"),
            elems: None,
            ptr_slots: Cow::Owned(layout.ptr_offsets()),
            tagged_slots: Cow::Owned(layout.tagged_offsets()),
            slot_kinds: Cow::Owned(layout.fields().iter().map(|slot| slot.kind).collect()),
            type_id: layout.descriptor().0,
            finalizer: None,
        }
    }

    fn validate(&self) -> Result<(), DescriptorError> {
        if (self.size as usize) < HEADER_BYTES {
            return Err(DescriptorError::SmallerThanHeader(self.size));
        }
        let has_slots =
            !self.ptr_slots.is_empty() || !self.tagged_slots.is_empty() || self.elems.is_some();
        if matches!(self.trace, Trace::Rust(_)) && has_slots {
            return Err(DescriptorError::RustWithSlots);
        }
        let slots = [
            (&self.ptr_slots, WORD_BYTES),
            (&self.tagged_slots, size_of::<TaggedSlot>()),
        ];
        let bad = slots.iter().find_map(|(offsets, width)| {
            offsets
                .iter()
                .copied()
                .find(|&offset| !self.slot_fits(offset as usize, *width))
        });
        bad.map_or(Ok(()), |offset| Err(DescriptorError::BadSlot(offset)))
    }

    fn slot_fits(&self, offset: usize, width: usize) -> bool {
        offset >= HEADER_BYTES
            && offset.is_multiple_of(WORD_BYTES)
            && offset + width <= self.size as usize
    }
}

/// The descriptors below [`FIRST_INSTANCE_DESC`], fixed by the object format:
/// built once per process and shared by every heap.
fn fixed_descriptors() -> &'static [Option<Descriptor>] {
    static FIXED: std::sync::LazyLock<Vec<Option<Descriptor>>> = std::sync::LazyLock::new(|| {
        let mut fixed = vec![None; FIRST_INSTANCE_DESC.0 as usize];
        fixed[OBJECT_DESC.0 as usize] = Some(OBJECT_DESCRIPTOR);
        for (slot, id) in LIST_DESCS {
            fixed[id.0 as usize] = Some(Descriptor::list(slot));
        }
        for (elem, id) in BACKING_DESCS {
            fixed[id.0 as usize] = Some(Descriptor::backing(elem));
        }
        for (key, value, id) in map_descriptors() {
            fixed[id.0 as usize] = Some(Descriptor::map(key, value));
        }
        fixed[STR_DESC.0 as usize] = Some(Descriptor::str());
        fixed
    });
    &FIXED
}

/// Every descriptor a heap knows, by [`DescriptorId`].
///
/// IDs below [`FIRST_INSTANCE_DESC`] are fixed by the object format: entry
/// [`OBJECT_DESC`] is always the VM's `Object`. Instance layouts are added at
/// run time, when a class allocates its first instance, and are interned by
/// their slots so every class with the same layout shares one ID: compiled
/// code and the interpreter derive the same ID from the same slot kinds.
///
/// Reads take no lock: entries live in segments that never move, each set
/// once. Additions take `interned`.
pub struct DescriptorTable {
    fixed: &'static [Option<Descriptor>],
    segments: [OnceLock<Box<[OnceLock<Descriptor>]>>; SEGMENTS],
    /// Instance layouts by their slots, and the next free dynamic index.
    interned: Mutex<(HashMap<InstanceKey, DescriptorId>, u32)>,
}

/// An instance layout's identity: its slot kinds and whether it carries type
/// arguments.
type InstanceKey = (Vec<SlotKind>, bool);

/// Segment `k` holds `FIRST_SEGMENT << k` entries.
const SEGMENTS: usize = 24;
const FIRST_SEGMENT: u32 = 64;

/// The segment and index of dynamic entry `index`.
fn locate(index: u32) -> (usize, usize) {
    let shifted = index + FIRST_SEGMENT;
    let segment = (u32::BITS - 1 - shifted.leading_zeros()) - FIRST_SEGMENT.trailing_zeros();
    let start = (FIRST_SEGMENT << segment) - FIRST_SEGMENT;
    (segment as usize, (index - start) as usize)
}

impl DescriptorTable {
    pub(crate) fn new() -> Self {
        let table = Self {
            fixed: fixed_descriptors(),
            segments: [const { OnceLock::new() }; SEGMENTS],
            interned: Mutex::new((HashMap::new(), 0)),
        };
        // Every program allocates instances, so the first segment is made
        // with the heap rather than inside its first instance allocation.
        table.segment(0);
        table
    }

    /// Segment `segment`, made on first use.
    fn segment(&self, segment: usize) -> &[OnceLock<Descriptor>] {
        self.segments[segment].get_or_init(|| {
            (0..(FIRST_SEGMENT << segment))
                .map(|_| OnceLock::new())
                .collect()
        })
    }

    /// Add `descriptor` at the next dynamic ID.
    pub fn register(&self, descriptor: Descriptor) -> Result<DescriptorId, DescriptorError> {
        descriptor.validate()?;
        let mut interned = self.interned.lock();
        Ok(self.push(&mut interned.1, descriptor))
    }

    /// The ID of the instance layout with `kinds` (and type arguments when
    /// `has_type_args`), registering it on first use.
    pub fn intern_instance(&self, kinds: &[SlotKind], has_type_args: bool) -> InstanceLayout {
        let mut interned = self.interned.lock();
        let (by_key, next) = &mut *interned;
        let key = (kinds.to_vec(), has_type_args);
        if let Some(&id) = by_key.get(&key) {
            return InstanceLayout::new(id, kinds, has_type_args);
        }
        let id = DescriptorId(FIRST_INSTANCE_DESC.0 + *next);
        let layout = InstanceLayout::new(id, kinds, has_type_args);
        let descriptor = Descriptor::for_instance(&layout);
        descriptor
            .validate()
            .expect("an instance layout makes a valid descriptor");
        let pushed = self.push(next, descriptor);
        debug_assert_eq!(pushed, id);
        by_key.insert(key, id);
        layout
    }

    fn push(&self, next: &mut u32, descriptor: Descriptor) -> DescriptorId {
        let index = *next;
        let (segment, slot) = locate(index);
        let entries = self.segment(segment);
        assert!(
            entries[slot].set(descriptor).is_ok(),
            "descriptor slot set twice"
        );
        *next += 1;
        DescriptorId(FIRST_INSTANCE_DESC.0 + index)
    }

    #[inline]
    pub fn get(&self, id: DescriptorId) -> &Descriptor {
        if id.0 < FIRST_INSTANCE_DESC.0 {
            return self.fixed[id.0 as usize]
                .as_ref()
                .unwrap_or_else(|| panic!("no fixed descriptor {id:?}"));
        }
        let (segment, slot) = locate(id.0 - FIRST_INSTANCE_DESC.0);
        self.segments[segment]
            .get()
            .and_then(|entries| entries[slot].get())
            .unwrap_or_else(|| panic!("no descriptor {id:?}"))
    }

    /// Fixed and dynamic descriptors registered.
    pub fn len(&self) -> usize {
        let fixed = self.fixed.iter().flatten().count();
        fixed + self.interned.lock().1 as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Bytes from a cell's start to a Rust value inside it.
pub(crate) const RUST_PAYLOAD_OFFSET: usize = HEADER_BYTES;
/// A VM `Object` cell: header, then the enum.
pub(crate) const OBJECT_CELL_BYTES: usize = granule_align(HEADER_BYTES + size_of::<Object>());
// The payload must be aligned for `Object` and stay in the cell's first
// granule, so a `HeapPtr` to it masks back to the cell.
const _: () = assert!(RUST_PAYLOAD_OFFSET.is_multiple_of(align_of::<Object>()));
const _: () = assert!(RUST_PAYLOAD_OFFSET < GRANULE_BYTES);

static OBJECT_HOOKS: RustHooks = RustHooks {
    trace: crate::object_trace::trace_object_hook,
    drop: crate::object_trace::drop_object_hook,
    measure: crate::object_trace::measure_object_hook,
};

const OBJECT_DESCRIPTOR: Descriptor = Descriptor::rust::<Object>(&OBJECT_HOOKS, 0);

/// Push a pointer word found in a slot, ignoring zero.
fn push_word(word: usize, out: &mut Vec<HeapPtr>) {
    if word != 0 {
        out.push(crate::object_trace::heap_ptr_at(word));
    }
}

/// Push the pointer a slot holds, if any.
///
/// # Safety
/// `slot` addresses a readable slot of `kind`.
unsafe fn push_slot(slot: usize, kind: ElemKind, out: &mut Vec<HeapPtr>) {
    // SAFETY: forwarded to the caller.
    if let Some(bits) =
        unsafe { bex_vm_types::object_format::slot_pointer(slot as *const u8, kind) }
    {
        push_word(bits as usize, out);
    }
}

/// Push every pointer of the slot-described cell at `cell`, `cell_bytes` long.
///
/// # Safety
/// `cell` is an allocated cell laid out by `descriptor`.
pub(crate) unsafe fn trace_slots(
    descriptor: &Descriptor,
    cell: usize,
    cell_bytes: usize,
    out: &mut Vec<HeapPtr>,
) {
    let slots = [
        (&descriptor.ptr_slots, ElemKind::Ptr),
        (&descriptor.tagged_slots, ElemKind::Tagged16),
    ];
    for (offsets, kind) in slots {
        for &offset in offsets.iter() {
            // SAFETY: validated in bounds when the descriptor was registered.
            unsafe { push_slot(cell + offset as usize, kind, out) };
        }
    }
    let Some(elems) = descriptor.elems else {
        return;
    };
    if elems == ElemKind::Raw8 {
        return;
    }
    for offset in (descriptor.size as usize..cell_bytes).step_by(elems.stride()) {
        // SAFETY: each element lies inside the cell.
        unsafe { push_slot(cell + offset, elems, out) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(ptr_slots: &'static [u32], tagged_slots: &'static [u32]) -> Descriptor {
        Descriptor {
            kind: Kind::Instance,
            trace: Trace::Slots,
            size: 32,
            elems: None,
            ptr_slots: Cow::Borrowed(ptr_slots),
            tagged_slots: Cow::Borrowed(tagged_slots),
            slot_kinds: Cow::Borrowed(&[SlotKind::Ptr, SlotKind::Ptr, SlotKind::Raw(RawKind::Int)]),
            type_id: 1,
            finalizer: None,
        }
    }

    #[test]
    fn registration_validates_slot_offsets() {
        let table = DescriptorTable::new();
        assert_eq!(
            table.register(instance(&[8, 16], &[])),
            Ok(FIRST_INSTANCE_DESC)
        );
        assert_eq!(
            table.register(instance(&[0], &[])),
            Err(DescriptorError::BadSlot(0)),
            "overlaps the header"
        );
        assert_eq!(
            table.register(instance(&[], &[24])),
            Err(DescriptorError::BadSlot(24)),
            "overruns the object"
        );
        assert_eq!(table.len(), 1 + 5 + 3 + 25 + 1 + 1);
    }

    /// Fields are laid out after the header in declaration order, each in
    /// the slot PR B's `SlotKind::of_type` gives its type.
    #[test]
    fn field_layout_follows_the_slot_table() {
        let union = RealizedTy::Union(Box::new([RealizedTy::Int, RealizedTy::String]));
        let fields = [
            RealizedTy::Int,
            RealizedTy::String,
            union,
            RealizedTy::Float,
        ];
        let descriptor = Descriptor::for_fields(Kind::Instance, &fields, 7);
        assert_eq!(
            *descriptor.slot_kinds,
            [
                SlotKind::Raw(RawKind::Int),
                SlotKind::Ptr,
                SlotKind::Tagged16,
                SlotKind::Raw(RawKind::Float),
            ]
        );
        assert_eq!(*descriptor.ptr_slots, [16]);
        assert_eq!(*descriptor.tagged_slots, [24]);
        assert_eq!(descriptor.size, 48);
        let table = DescriptorTable::new();
        assert!(table.register(descriptor).is_ok());
    }
}
