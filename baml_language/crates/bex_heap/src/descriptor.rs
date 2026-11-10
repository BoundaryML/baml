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

use std::borrow::Cow;

pub use bex_vm_types::layout::{DescriptorId, ElemKind, Kind, RawKind, SlotKind};
use bex_vm_types::{HeapPtr, Meter, Object, RealizedTy};

use crate::space::{GRANULE_BYTES, granule_align};
/// Reflection identity of the type a descriptor lays out.
pub type TypeId = u32;
/// Index of the finalizer a descriptor's objects run when they die.
pub type FinalizerId = u32;

/// The first eight bytes of every cell.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub descriptor: DescriptorId,
    /// Reserved for the object format: cleanup latch, finalizer registered,
    /// pinned. The collector does not read them.
    pub flags: u16,
    /// Cell size in granules for a small cell; zero for a large one, whose
    /// size the large-object space records.
    granules: u16,
}

pub(crate) const HEADER_BYTES: usize = size_of::<Header>();
const _: () = assert!(HEADER_BYTES == 8);

impl Header {
    pub(crate) fn new(descriptor: DescriptorId, cell_bytes: usize, large: bool) -> Self {
        let granules = if large {
            0
        } else {
            u16::try_from(cell_bytes / GRANULE_BYTES).expect("small cells fit a u16 granule count")
        };
        Self {
            descriptor,
            flags: 0,
            granules,
        }
    }

    /// The header of `cell`.
    ///
    /// # Safety
    /// `cell` is an allocated cell.
    #[inline]
    pub(crate) unsafe fn at<'a>(cell: usize) -> &'a Self {
        // SAFETY: forwarded to the caller.
        unsafe { &*(cell as *const Self) }
    }

    /// Size of a small cell. Zero for a large one.
    #[inline]
    pub(crate) fn cell_bytes(&self) -> usize {
        usize::from(self.granules) * GRANULE_BYTES
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

/// The tag of a [`TaggedSlot`].
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaggedKind {
    Locked = 0,
    Ptr = 1,
    Int = 2,
    Float = 3,
    Bool = 4,
    Null = 5,
}

/// A slot that may hold a pointer or a scalar: `SafeValue` in the design.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TaggedSlot {
    pub kind: u8,
    pub _pad: [u8; 7],
    pub payload: u64,
}

const _: () = assert!(size_of::<TaggedSlot>() == 16);

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

/// Every descriptor a heap knows, by [`DescriptorId`]. Entry [`OBJECT_DESC`] is
/// always the VM's `Object`.
pub struct DescriptorTable {
    descriptors: Vec<Descriptor>,
}

impl DescriptorTable {
    pub(crate) fn new() -> Self {
        Self {
            descriptors: vec![OBJECT_DESCRIPTOR],
        }
    }

    /// Add `descriptor`, returning its ID.
    pub fn register(&mut self, descriptor: Descriptor) -> Result<DescriptorId, DescriptorError> {
        descriptor.validate()?;
        let id = u32::try_from(self.descriptors.len()).expect("descriptor table overflow");
        self.descriptors.push(descriptor);
        Ok(DescriptorId(id))
    }

    #[inline]
    pub fn get(&self, id: DescriptorId) -> &Descriptor {
        &self.descriptors[id.0 as usize]
    }

    pub fn len(&self) -> usize {
        self.descriptors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }
}

/// The descriptor of every VM `Object` cell.
pub const OBJECT_DESC: DescriptorId = DescriptorId(0);

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

/// Push a tagged slot's payload when it holds a pointer.
///
/// # Safety
/// `slot` addresses a readable [`TaggedSlot`].
unsafe fn push_tagged(slot: usize, out: &mut Vec<HeapPtr>) {
    // SAFETY: forwarded to the caller.
    let slot = unsafe { &*(slot as *const TaggedSlot) };
    if slot.kind == TaggedKind::Ptr as u8 {
        push_word(slot.payload as usize, out);
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
    for &offset in descriptor.ptr_slots.iter() {
        // SAFETY: validated in bounds when the descriptor was registered.
        push_word(unsafe { *((cell + offset as usize) as *const usize) }, out);
    }
    for &offset in descriptor.tagged_slots.iter() {
        // SAFETY: validated in bounds when the descriptor was registered.
        unsafe { push_tagged(cell + offset as usize, out) };
    }
    let Some(elems) = descriptor.elems else {
        return;
    };
    let elements = (descriptor.size as usize..cell_bytes).step_by(elems.stride());
    match elems {
        ElemKind::Raw8 => {}
        // SAFETY: each element lies inside the cell.
        ElemKind::Ptr => elements.for_each(|offset| {
            push_word(unsafe { *((cell + offset) as *const usize) }, out);
        }),
        // SAFETY: each element lies inside the cell.
        ElemKind::Tagged16 => {
            elements.for_each(|offset| unsafe { push_tagged(cell + offset, out) })
        }
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
        let mut table = DescriptorTable::new();
        assert_eq!(table.register(instance(&[8, 16], &[])), Ok(DescriptorId(1)));
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
        assert_eq!(table.len(), 2);
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
        let mut table = DescriptorTable::new();
        assert!(table.register(descriptor).is_ok());
    }
}
