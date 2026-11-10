//! Object descriptors: how the collector reads a cell.
//!
//! Every cell starts with a [`Header`] naming its [`Descriptor`] by index in
//! the heap's [`DescriptorTable`]. A descriptor says where the cell's
//! pointers are, either as byte offsets of pointer and tagged slots (the
//! shared object format of the runtime design) or, for an object that is
//! still a Rust value, as hooks into Rust code ([`Kind::Rust`]). Descriptor
//! IDs never change after allocation.
//!
//! The VM's `Object` enum is one such Rust value: every VM allocation is a
//! cell of descriptor [`OBJECT_DESC`], whose hooks call the enum-specific
//! tracing in [`crate::object_trace`].

use bex_vm_types::{HeapPtr, Meter, Object};

use crate::space::{GRANULE_BYTES, granule_align};

/// Index into a heap's [`DescriptorTable`].
pub type DescId = u32;
/// Reflection identity of the type a descriptor lays out.
pub type TypeId = u32;
/// Index of the finalizer a descriptor's objects run when they die.
pub type FinalizerId = u32;

/// The first eight bytes of every cell.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub descriptor: DescId,
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
    pub(crate) fn new(descriptor: DescId, cell_bytes: usize, large: bool) -> Self {
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

/// What kind of object a descriptor lays out. The slot-described kinds are
/// the names PR B's `bex_vm_types::layout::Kind` uses.
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Instance,
    List,
    Backing,
    Map,
    Str,
    Closure,
    Future,
    Bytes,
    Opaque,
    /// A Rust value placed after the header. Its pointers are found, its
    /// destructor run and its outside storage measured by Rust code.
    Rust(&'static RustHooks),
}

/// Rust code for a [`Kind::Rust`] object. Each takes a pointer to the value,
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

/// How repeated elements after an object's fixed part are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElemKind {
    /// Eight raw bytes: no pointers.
    Raw8,
    /// An eight-byte pointer, zero for none.
    Ptr,
    /// A [`TaggedSlot`].
    Tagged16,
}

impl ElemKind {
    pub const fn stride(self) -> usize {
        match self {
            Self::Raw8 | Self::Ptr => 8,
            Self::Tagged16 => size_of::<TaggedSlot>(),
        }
    }
}

/// What a raw slot holds, so the interpreter can box it back into a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawKind {
    Int,
    Float,
    Bool,
}

/// How the interpreter reads one declared field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    Raw(RawKind),
    Ptr,
    Tagged16,
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
#[derive(Clone, Copy, Debug)]
pub struct Descriptor {
    pub kind: Kind,
    /// Bytes from the cell start (header included): the whole object for a
    /// fixed object, the part before the first element when `elems` is set.
    pub size: u32,
    /// Elements repeat from `size` to the end of the cell.
    pub elems: Option<ElemKind>,
    /// Byte offsets, from the cell start, of pointer slots.
    pub ptr_slots: &'static [u32],
    /// Byte offsets, from the cell start, of [`TaggedSlot`]s.
    pub tagged_slots: &'static [u32],
    /// The interpreter's view, in declaration order.
    pub slot_kinds: &'static [SlotKind],
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

impl Descriptor {
    /// A descriptor for a Rust value of type `T`.
    pub const fn rust<T>(hooks: &'static RustHooks, type_id: TypeId) -> Self {
        Self {
            kind: Kind::Rust(hooks),
            size: (HEADER_BYTES + size_of::<T>()) as u32,
            elems: None,
            ptr_slots: &[],
            tagged_slots: &[],
            slot_kinds: &[],
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
        if matches!(self.kind, Kind::Rust(_)) && has_slots {
            return Err(DescriptorError::RustWithSlots);
        }
        let slots = [
            (self.ptr_slots, 8),
            (self.tagged_slots, size_of::<TaggedSlot>()),
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
        offset >= HEADER_BYTES && offset.is_multiple_of(8) && offset + width <= self.size as usize
    }
}

/// Every descriptor a heap knows, by [`DescId`]. Entry [`OBJECT_DESC`] is
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
    pub fn register(&mut self, descriptor: Descriptor) -> Result<DescId, DescriptorError> {
        descriptor.validate()?;
        let id = DescId::try_from(self.descriptors.len()).expect("descriptor table overflow");
        self.descriptors.push(descriptor);
        Ok(id)
    }

    #[inline]
    pub fn get(&self, id: DescId) -> &Descriptor {
        &self.descriptors[id as usize]
    }

    pub fn len(&self) -> usize {
        self.descriptors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }
}

/// The descriptor of every VM `Object` cell.
pub const OBJECT_DESC: DescId = 0;

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
    for &offset in descriptor.ptr_slots {
        // SAFETY: validated in bounds when the descriptor was registered.
        push_word(unsafe { *((cell + offset as usize) as *const usize) }, out);
    }
    for &offset in descriptor.tagged_slots {
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

    #[test]
    fn registration_validates_slot_offsets() {
        let mut table = DescriptorTable::new();
        let ok = Descriptor {
            kind: Kind::Instance,
            size: 32,
            elems: None,
            ptr_slots: &[8, 16],
            tagged_slots: &[],
            slot_kinds: &[SlotKind::Ptr, SlotKind::Ptr, SlotKind::Raw(RawKind::Int)],
            type_id: 1,
            finalizer: None,
        };
        assert_eq!(table.register(ok), Ok(1));
        let overlaps_header = Descriptor {
            ptr_slots: &[0],
            ..ok
        };
        assert_eq!(
            table.register(overlaps_header),
            Err(DescriptorError::BadSlot(0))
        );
        let overruns = Descriptor {
            tagged_slots: &[24],
            ..ok
        };
        assert_eq!(table.register(overruns), Err(DescriptorError::BadSlot(24)));
        assert_eq!(table.len(), 2);
    }
}
