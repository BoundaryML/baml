//! Class instances in the shared object format: a header, then the instance's
//! slots inline, in one allocation.
//!
//! Replaces the old `Instance` (`Object::Instance`: class pointer, boxed type
//! arguments and a separate `Vec<AtomicValueSlot>` of tagged fields, plus a
//! `CleanupLatch`).
//!
//! ```text
//! #[repr(C)]
//! struct Instance {
//!     hdr: Header,              // descriptor = the class layout's; flags bit 0 = cleaned
//!     class: AtomicU64,         // pointer slot: the `Object::Class`
//!     type_args: AtomicU64,     // pointer slot, generic classes only: an `Object::Type`
//!                               // whose type is `Class<args...>`
//!     fields: [slot],           // declaration order, each slot `SlotKind::of_type`'s
//! }
//! ```
//!
//! The class pointer is a slot, not only the descriptor's: a class created at
//! run time is collectible, and its instances keep it alive.

#![allow(unsafe_code)]

use crate::{
    HeapPtr, Object, RealizedTy, RuntimeTy, Value,
    layout::{DescriptorId, ElemKind, SlotKind},
    object_format::{
        Access, Format, HEADER_BYTES, Header, SlotMismatch, SlotValue, TAGGED_ALIGN, read_slot,
        write_float_slot, write_slot,
    },
    types::Class,
};

/// Bytes of a raw or pointer slot.
const WORD: u32 = 8;
/// Offset, from the header, of the class pointer.
const CLASS_OFFSET: u32 = HEADER_BYTES as u32;
/// Offset, from the header, of a generic instance's type pointer.
const TYPE_ARGS_OFFSET: u32 = CLASS_OFFSET + WORD;

/// One declared field's slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldSlot {
    /// Bytes from the header.
    pub offset: u32,
    pub kind: SlotKind,
}

/// How a class's instances are laid out. Built once per class, when its first
/// instance is allocated, and cached on the [`Class`].
#[derive(Clone, Debug)]
pub struct InstanceLayout {
    descriptor: DescriptorId,
    has_type_args: bool,
    fields: Box<[FieldSlot]>,
    /// Bytes of the whole object, header included.
    size: u32,
}

impl InstanceLayout {
    /// The slot kinds of `class`'s declared fields, and whether its instances
    /// carry type arguments. A generic parameter, or any type that is not
    /// fully known, is tagged.
    pub fn plan(class: &Class) -> (Vec<SlotKind>, bool) {
        let kinds = class
            .fields
            .iter()
            .map(|field| field_slot_kind(&field.field_type))
            .collect();
        (kinds, class.generic_param_count > 0)
    }

    /// The layout of `kinds` after the class (and type) pointers, registered
    /// as `descriptor`.
    pub fn new(descriptor: DescriptorId, kinds: &[SlotKind], has_type_args: bool) -> Self {
        let mut next = if has_type_args {
            TYPE_ARGS_OFFSET + WORD
        } else {
            TYPE_ARGS_OFFSET
        };
        let fields = kinds
            .iter()
            .map(|&kind| {
                // A tagged slot sits 16-byte aligned for its single-copy
                // atomic path; the gap before it is padding.
                if kind == SlotKind::Tagged16 {
                    next = next.next_multiple_of(TAGGED_ALIGN as u32);
                }
                let offset = next;
                next += stride(kind);
                FieldSlot { offset, kind }
            })
            .collect();
        Self {
            descriptor,
            has_type_args,
            fields,
            size: next,
        }
    }

    pub fn descriptor(&self) -> DescriptorId {
        self.descriptor
    }

    pub fn has_type_args(&self) -> bool {
        self.has_type_args
    }

    pub fn fields(&self) -> &[FieldSlot] {
        &self.fields
    }

    /// Bytes of the whole object, header included.
    pub fn size(&self) -> usize {
        self.size as usize
    }

    /// Bytes of padding before 16-byte-aligned tagged fields.
    pub fn padding(&self) -> usize {
        let used: u32 = self.fields.iter().map(|slot| stride(slot.kind)).sum();
        let hidden = if self.has_type_args { 2 * WORD } else { WORD };
        (self.size - HEADER_BYTES as u32 - hidden - used) as usize
    }

    /// Offsets of every pointer slot, the class and type pointers included.
    pub fn ptr_offsets(&self) -> Vec<u32> {
        let hidden = if self.has_type_args {
            &[CLASS_OFFSET, TYPE_ARGS_OFFSET][..]
        } else {
            &[CLASS_OFFSET][..]
        };
        let fields = self.offsets_of(ElemKind::Ptr);
        hidden.iter().copied().chain(fields).collect()
    }

    /// Offsets of every tagged slot.
    pub fn tagged_offsets(&self) -> Vec<u32> {
        self.offsets_of(ElemKind::Tagged16).collect()
    }

    fn offsets_of(&self, kind: ElemKind) -> impl Iterator<Item = u32> + '_ {
        self.fields
            .iter()
            .filter(move |slot| slot.kind.elem_kind() == kind)
            .map(|slot| slot.offset)
    }
}

fn stride(kind: SlotKind) -> u32 {
    u32::try_from(kind.elem_kind().stride()).expect("a slot stride fits u32")
}

/// The slot a field declared as `ty` is stored in. A type that still names a
/// generic parameter is tagged.
fn field_slot_kind(ty: &RuntimeTy) -> SlotKind {
    RealizedTy::try_from(ty).map_or(SlotKind::Tagged16, |realized| SlotKind::of_type(&realized))
}

/// A class instance on the heap.
///
/// Valid while the instance is: like [`HeapPtr::get`], the references it
/// hands out are `'static` and the caller keeps the object alive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InstanceRef(HeapPtr);

impl InstanceRef {
    /// The instance `ptr` names, if it is one.
    ///
    /// # Safety
    /// `ptr` names a live object.
    #[inline]
    pub unsafe fn of(ptr: HeapPtr) -> Option<Self> {
        // SAFETY: forwarded to the caller.
        (unsafe { ptr.format() } == Format::Instance).then_some(Self(ptr))
    }

    /// # Safety
    /// `ptr` names a live instance.
    #[inline]
    pub unsafe fn new_unchecked(ptr: HeapPtr) -> Self {
        debug_assert_eq!(
            // SAFETY: forwarded to the caller.
            unsafe { ptr.format() },
            Format::Instance
        );
        Self(ptr)
    }

    /// Fill a fresh, zeroed instance cell: its class and type pointers, and
    /// null in every tagged field. The fields are stored next, before the
    /// instance is published.
    ///
    /// # Safety
    /// `ptr` names a freshly allocated cell of `class`'s layout, and `type_args`
    /// is an `Object::Type` exactly when the layout has type arguments.
    pub unsafe fn init(ptr: HeapPtr, class: HeapPtr, type_args: Option<HeapPtr>) -> Self {
        let instance = Self(ptr);
        let class_value = Value::object(class);
        // SAFETY: the class slot is inside the fresh cell.
        unsafe { instance.write_hidden(CLASS_OFFSET, class_value) };
        if let Some(type_args) = type_args {
            // SAFETY: the layout has a type slot.
            unsafe { instance.write_hidden(TYPE_ARGS_OFFSET, Value::object(type_args)) };
        }
        // A zeroed tagged slot reads as held (tag 0); every one starts null.
        for slot in instance.layout().fields.iter() {
            if slot.kind == SlotKind::Tagged16 {
                // SAFETY: the layout's slot lies inside the fresh cell.
                unsafe {
                    write_slot(
                        instance.addr(slot.offset),
                        slot.kind,
                        Value::NULL,
                        Access::ThreadLocal,
                    )
                }
                .expect("null fits a tagged slot");
            }
        }
        instance
    }

    unsafe fn write_hidden(self, offset: u32, value: Value) {
        // SAFETY: forwarded to the caller.
        unsafe { write_slot(self.addr(offset), SlotKind::Ptr, value, Access::ThreadLocal) }
            .expect("a pointer fits a pointer slot");
    }

    #[inline]
    pub fn ptr(self) -> HeapPtr {
        self.0
    }

    #[inline]
    fn addr(self, offset: u32) -> *const u8 {
        // SAFETY: offsets are within the object; the header precedes the
        // pointer.
        unsafe {
            self.0
                .as_ptr()
                .cast::<u8>()
                .sub(HEADER_BYTES)
                .add(offset as usize)
        }
    }

    #[inline]
    fn hidden(self, offset: u32) -> HeapPtr {
        // SAFETY: the hidden slots are pointers written at allocation.
        let value = unsafe { read_slot(self.addr(offset), SlotKind::Ptr, Access::ThreadLocal) };
        value
            .expect_value()
            .as_object_ptr()
            .expect("an instance's class and type slots are set")
    }

    /// The `Object::Class` this is an instance of.
    #[inline]
    pub fn class(self) -> HeapPtr {
        self.hidden(CLASS_OFFSET)
    }

    #[inline]
    pub fn class_def(self) -> &'static Class {
        // SAFETY: the instance keeps its class alive.
        match unsafe { self.class().get() } {
            Object::Class(class) => class,
            other => unreachable!("an instance's class slot holds {other}"),
        }
    }

    #[inline]
    pub fn layout(self) -> &'static InstanceLayout {
        self.class_def()
            .layout
            .get()
            .expect("a class has its layout once it has an instance")
    }

    /// Class type arguments, De Bruijn-ordered like the class's generic
    /// parameters. Empty for a non-generic class.
    pub fn type_args(self) -> &'static [RealizedTy] {
        if !self.layout().has_type_args {
            return &[];
        }
        // SAFETY: the instance keeps its type object alive.
        match unsafe { self.hidden(TYPE_ARGS_OFFSET).get() } {
            Object::Type(value) => match &value.ty {
                RealizedTy::Class(_, args) => args,
                other => unreachable!("an instance's type slot holds {other:?}"),
            },
            other => unreachable!("an instance's type slot holds {other}"),
        }
    }

    #[inline]
    pub fn header(self) -> &'static Header {
        // SAFETY: a live instance has a header.
        unsafe { self.0.header() }
    }

    #[inline]
    pub fn field_len(self) -> usize {
        self.layout().fields.len()
    }

    #[inline]
    fn slot(self, idx: usize) -> Option<FieldSlot> {
        self.layout().fields.get(idx).copied()
    }

    /// Read field `idx` with `access`, or `None` past the last field.
    #[inline]
    pub fn try_load_with(self, idx: usize, access: Access) -> Option<SlotValue> {
        let slot = self.slot(idx)?;
        // SAFETY: the layout's slot lies inside the live instance.
        Some(unsafe { read_slot(self.addr(slot.offset), slot.kind, access) })
    }

    #[inline]
    pub fn try_load(self, idx: usize) -> Option<SlotValue> {
        self.try_load_with(idx, Access::Shared)
    }

    /// # Panics
    /// Past the last field.
    #[inline]
    #[track_caller]
    pub fn load(self, idx: usize) -> SlotValue {
        self.try_load(idx)
            .unwrap_or_else(|| panic!("field {idx} of a {}-field instance", self.field_len()))
    }

    /// Store `value` to field `idx` with `access`. `Err(None)` past the last
    /// field; `Err(Some(_))` when the value does not fit the field's slot.
    #[inline]
    pub fn try_store_with(
        self,
        idx: usize,
        value: Value,
        access: Access,
    ) -> Result<(), Option<SlotMismatch>> {
        let slot = self.slot(idx).ok_or(None)?;
        // SAFETY: the layout's slot lies inside the live instance.
        unsafe { write_slot(self.addr(slot.offset), slot.kind, value, access) }.map_err(Some)
    }

    /// Store `value` to field `idx`.
    ///
    /// # Panics
    /// Past the last field, or when the value does not fit the field's
    /// declared type.
    #[inline]
    #[track_caller]
    pub fn store(self, idx: usize, value: Value) {
        self.store_with(idx, value, Access::Shared);
    }

    /// [`Self::store`] with `access`.
    #[inline]
    #[track_caller]
    pub fn store_with(self, idx: usize, value: Value, access: Access) {
        if let Err(error) = self.try_store_with(idx, value, access) {
            self.store_failed(idx, error);
        }
    }

    /// Store an unboxed float to field `idx`.
    #[track_caller]
    pub fn store_float(self, idx: usize, float: f64, access: Access) {
        let Some(slot) = self.slot(idx) else {
            self.store_failed(idx, None);
        };
        // SAFETY: the layout's slot lies inside the live instance.
        let stored = unsafe { write_float_slot(self.addr(slot.offset), slot.kind, float, access) };
        if let Err(error) = stored {
            self.store_failed(idx, Some(error));
        }
    }

    /// Store a slot value read from another slot.
    #[track_caller]
    pub fn store_slot_value(self, idx: usize, value: SlotValue, access: Access) {
        match value {
            SlotValue::Value(value) => self.store_with(idx, value, access),
            SlotValue::Float(float) => self.store_float(idx, float, access),
        }
    }

    #[cold]
    #[track_caller]
    fn store_failed(self, idx: usize, error: Option<SlotMismatch>) -> ! {
        let class = self.class_def().name();
        match error {
            None => panic!(
                "{class}: field {idx} of a {}-field instance",
                self.field_len()
            ),
            Some(error) => {
                let field = &self.class_def().fields[idx].name;
                panic!("{class}.{field}: {error}")
            }
        }
    }

    /// Every field, in declaration order.
    pub fn fields(self) -> impl ExactSizeIterator<Item = SlotValue> {
        (0..self.field_len()).map(move |idx| self.load(idx))
    }

    /// BEP-042 run-once latch: `true` for the first `cleanup` of this
    /// instance, which runs the body; `false` once cleaned.
    #[inline]
    pub fn begin_cleanup(self) -> bool {
        !self.header().test_and_set(Header::CLEANED)
    }

    #[inline]
    pub fn is_cleaned(self) -> bool {
        self.header().is_set(Header::CLEANED)
    }
}

impl std::fmt::Display for InstanceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<instance of {:p}>", self.class().as_ptr())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::RawKind;

    #[test]
    fn fields_follow_the_class_and_type_pointers() {
        let kinds = [
            SlotKind::Raw(RawKind::Int),
            SlotKind::Tagged16,
            SlotKind::Ptr,
        ];
        // The tagged field is padded from 24 to 32, 16-byte aligned.
        let plain = InstanceLayout::new(DescriptorId(64), &kinds, false);
        assert_eq!(
            plain.fields().iter().map(|f| f.offset).collect::<Vec<_>>(),
            [16, 32, 48]
        );
        assert_eq!(plain.size(), 56);
        assert_eq!(plain.padding(), 8);
        assert_eq!(plain.ptr_offsets(), [8, 48]);
        assert_eq!(plain.tagged_offsets(), [32]);

        // With a type pointer the tagged field lands aligned without padding.
        let generic = InstanceLayout::new(DescriptorId(65), &kinds, true);
        assert_eq!(generic.fields()[0].offset, 24);
        assert_eq!(generic.tagged_offsets(), [32]);
        assert_eq!(generic.ptr_offsets(), [8, 16, 48]);
        assert_eq!(generic.size(), 56);
        assert_eq!(generic.padding(), 0);
    }
}
