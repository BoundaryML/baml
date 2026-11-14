//! Class instances: one heap cell of 8-byte words, the header first and then
//! one slot per field at a fixed offset. A field of type `int`, `float` or
//! `bool` holds its raw value; every other field holds a tagged [`Value`].
//!
//! # Header
//!
//! Bits 1-2 of a cell's header are the cell's kind. Kind 0 is an instance:
//! the header is its class pointer's bits, with bit 0 the BEP-042 cleanup
//! latch. Other kinds are reserved for objects that move into cell spaces.
//!
#![allow(
    clippy::inline_always,
    reason = "every accessor is a compiled field access's fast path"
)]
//! Every slot is one atomic word, since an instance can be shared across
//! `spawn` fibers. A shared access orders like [`AtomicValueSlot`](crate::AtomicValueSlot)
//! whatever the slot's kind, so a field still publishes the writes before
//! it; an access escape analysis proved thread-local needs no ordering.

#![allow(unsafe_code)]

use std::{
    ptr::NonNull,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{Class, ClassField, HeapPtr, Object, RealizedTy, RuntimeTy, Value};

/// Bytes in one cell word: the header and every slot.
pub const WORD_BYTES: usize = size_of::<u64>();

/// The header bit set once an instance's `cleanup` has started.
const CLEANED: u64 = 0b001;

/// The header bits that are not the class pointer: the latch and the kind.
const HEADER_FLAGS: u64 = 0b111;

/// How an instance stores one field, fixed for a class by the field's
/// declared type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    /// A raw `i64`.
    Int,
    /// A raw `f64`, by its bits.
    Float,
    /// A raw `bool`: 0 or 1.
    Bool,
    /// A tagged [`Value`].
    Value,
}

impl SlotKind {
    /// The slot a field of type `ty` gets.
    #[must_use]
    #[inline(always)]
    pub fn of(ty: &RuntimeTy) -> Self {
        match ty {
            RuntimeTy::Int => Self::Int,
            RuntimeTy::Float => Self::Float,
            RuntimeTy::Bool => Self::Bool,
            _ => Self::Value,
        }
    }
}

/// Where an instance of a class keeps its fields. The one definition of an
/// instance's layout, read by the runtime, the collector and the Rust
/// emitter alike.
///
/// A generic class's instances carry their class type args in a hidden
/// [`SlotKind::Value`] slot after the header, naming an [`Object::Type`] of
/// the instantiated class.
#[derive(Clone, Copy)]
pub struct ClassLayout<'a> {
    fields: &'a [ClassField],
    first_field: usize,
}

impl<'a> ClassLayout<'a> {
    #[must_use]
    #[inline(always)]
    pub fn of(class: &'a Class) -> Self {
        Self {
            fields: &class.fields,
            first_field: 1 + usize::from(class.generic_param_count > 0),
        }
    }

    /// Words in an instance cell, the header included.
    #[must_use]
    #[inline(always)]
    pub fn words(self) -> usize {
        self.first_field + self.fields.len()
    }

    #[must_use]
    #[inline(always)]
    pub fn field_count(self) -> usize {
        self.fields.len()
    }

    #[must_use]
    #[inline(always)]
    pub fn kind(self, field: usize) -> SlotKind {
        SlotKind::of(&self.fields[field].field_type)
    }

    /// The word holding `field`.
    #[must_use]
    #[inline(always)]
    pub fn slot(self, field: usize) -> usize {
        self.first_field + field
    }

    /// The word holding the class type args, for a generic class.
    #[must_use]
    pub fn type_args_slot(self) -> Option<usize> {
        (self.first_field > 1).then_some(1)
    }

    /// Every word holding a [`Value`]: what the collector traces.
    pub fn value_slots(self) -> impl Iterator<Item = usize> + 'a {
        let fields = (0..self.fields.len())
            .filter(move |&field| self.kind(field) == SlotKind::Value)
            .map(move |field| self.slot(field));
        self.type_args_slot().into_iter().chain(fields)
    }
}

/// A field as an instance holds it: a value, or a raw float, which becomes
/// a [`Value`] only by boxing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Field {
    Value(Value),
    Float(f64),
}

impl Field {
    /// The value this field holds, unless it is a raw float.
    #[must_use]
    pub fn as_value(self) -> Option<Value> {
        match self {
            Self::Value(value) => Some(value),
            Self::Float(_) => None,
        }
    }
}

/// A value of the wrong kind for the slot it was stored into.
#[derive(Clone, Copy, Debug)]
pub struct SlotKindMismatch {
    pub field: usize,
    pub kind: SlotKind,
    pub value: Value,
}

/// A view of a live instance cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstanceRef {
    cell: NonNull<AtomicU64>,
}

impl InstanceRef {
    /// The instance `ptr` names, or `None` when it names an [`Object`].
    ///
    /// # Safety
    ///
    /// As for [`HeapPtr::get`]: the cell must be live.
    #[must_use]
    #[inline(always)]
    pub unsafe fn of(ptr: HeapPtr) -> Option<Self> {
        // SAFETY: the caller guarantees the cell is live.
        ptr.is_instance()
            .then(|| unsafe { Self::from_cell(ptr.cell()) })
    }

    /// The instance whose cell starts at `cell`.
    ///
    /// # Safety
    ///
    /// `cell` is a live instance cell, or one being built.
    #[must_use]
    #[inline(always)]
    pub unsafe fn from_cell(cell: *mut AtomicU64) -> Self {
        Self {
            cell: NonNull::new(cell).expect("a cell is never null"),
        }
    }

    /// The header of a fresh cell of `class`.
    #[must_use]
    #[inline(always)]
    pub fn header(class: HeapPtr) -> u64 {
        let bits = class.as_ptr() as u64;
        debug_assert_eq!(bits & HEADER_FLAGS, 0, "class {class:?} is misaligned");
        bits
    }

    /// The class pointer a header names, as a [`Value`] holds one.
    #[must_use]
    #[inline(always)]
    pub fn class_of_header(header: u64) -> HeapPtr {
        // SAFETY: a header holds its class pointer's bits.
        let class = unsafe { Value::from_bits(header & !HEADER_FLAGS) }.as_object_ptr();
        class.expect("an instance header names its class")
    }

    /// The `HeapPtr` naming this instance.
    #[must_use]
    #[inline(always)]
    pub fn ptr(self) -> HeapPtr {
        #[cfg(not(feature = "heap_debug"))]
        // SAFETY: `self` names an instance cell.
        let ptr = unsafe { HeapPtr::from_cell(self.cell.as_ptr()) };
        #[cfg(feature = "heap_debug")]
        // SAFETY: `self` names an instance cell.
        let ptr = unsafe { HeapPtr::from_cell(self.cell.as_ptr(), 0) };
        ptr
    }

    /// Word `slot` of the cell, the header being word 0.
    ///
    /// # Safety
    ///
    /// `slot` is within the cell: below its class layout's
    /// [`words`](ClassLayout::words).
    #[must_use]
    #[inline(always)]
    pub unsafe fn word(self, slot: usize) -> &'static AtomicU64 {
        // SAFETY: the cell is live, and the caller keeps `slot` within it.
        unsafe { &*self.cell.as_ptr().add(slot) }
    }

    #[inline(always)]
    fn header_word(self) -> &'static AtomicU64 {
        // SAFETY: every cell has its header.
        unsafe { self.word(0) }
    }

    #[must_use]
    #[inline(always)]
    pub fn class(self) -> HeapPtr {
        Self::class_of_header(self.header_word().load(Ordering::Relaxed))
    }

    /// Whether this is an instance of exactly `class`, from its header alone.
    #[must_use]
    #[inline(always)]
    pub fn is_of(self, class: HeapPtr) -> bool {
        self.header_word().load(Ordering::Relaxed) & !HEADER_FLAGS == Self::header(class)
    }

    /// Point the instance at `class`, keeping its flags: how an allocator
    /// writes a fresh cell's header, and how the collector moves a class.
    pub fn set_class(self, class: HeapPtr) {
        let flags = self.header_word().load(Ordering::Relaxed) & HEADER_FLAGS;
        self.header_word()
            .store(Self::header(class) | flags, Ordering::Relaxed);
    }

    /// The value in word `slot`.
    ///
    /// # Safety
    ///
    /// `slot` is a [`SlotKind::Value`] slot of the instance's layout.
    #[must_use]
    pub unsafe fn slot_value(self, slot: usize) -> Value {
        // SAFETY: the caller names a `Value` slot, which holds a live value's
        // bits.
        unsafe { Value::from_bits(self.word(slot).load(Ordering::Acquire)) }
    }

    /// Store `value` into word `slot`.
    ///
    /// # Safety
    ///
    /// As for [`Self::slot_value`].
    pub unsafe fn set_slot_value(self, slot: usize, value: Value) {
        // SAFETY: the caller names a slot within the cell.
        unsafe { self.word(slot) }.store(value.bits(), Ordering::Release);
    }

    #[must_use]
    #[inline(always)]
    pub fn class_object(self) -> &'static Class {
        // SAFETY: an instance keeps its class alive.
        match unsafe { self.class().get() } {
            Object::Class(class) => class,
            other => unreachable!("an instance's class is a {}", crate::ObjectType::of(other)),
        }
    }

    #[must_use]
    #[inline(always)]
    pub fn layout(self) -> ClassLayout<'static> {
        ClassLayout::of(self.class_object())
    }

    #[must_use]
    #[inline(always)]
    pub fn field_len(self) -> usize {
        self.layout().field_count()
    }

    /// Set the BEP-042 cleanup latch: `true` only for the first call, which
    /// runs the body. Set on entry, so a body that throws is not retried.
    #[must_use]
    pub fn begin_cleanup(self) -> bool {
        self.header_word().fetch_or(CLEANED, Ordering::AcqRel) & CLEANED == 0
    }

    #[must_use]
    pub fn is_cleaned(self) -> bool {
        self.header_word().load(Ordering::Acquire) & CLEANED != 0
    }

    /// The class type args the instance was built with; empty when its
    /// class is not generic or none were given.
    #[must_use]
    pub fn type_args(self) -> &'static [RealizedTy] {
        let Some(slot) = self.layout().type_args_slot() else {
            return &[];
        };
        // SAFETY: the layout says `slot` holds the type args value.
        let Some(ptr) = unsafe { self.slot_value(slot) }.as_object_ptr() else {
            return &[];
        };
        // SAFETY: the instance keeps its type args alive.
        match unsafe { ptr.get() } {
            Object::Type(ty) => match &ty.ty {
                RealizedTy::Class(_, args) => args,
                other => unreachable!("an instance's type args are a {other}"),
            },
            other => unreachable!(
                "an instance's type args are a {}",
                crate::ObjectType::of(other)
            ),
        }
    }

    /// `field`, ordered for an instance other threads can reach.
    #[must_use]
    #[inline(always)]
    pub fn load(self, field: usize) -> Field {
        self.load_ordered(field, Ordering::Acquire)
    }

    /// [`Self::load`] on an instance no other thread can reach.
    #[must_use]
    #[inline(always)]
    pub fn load_relaxed(self, field: usize) -> Field {
        self.load_ordered(field, Ordering::Relaxed)
    }

    #[inline(always)]
    fn load_ordered(self, field: usize, order: Ordering) -> Field {
        self.load_in(self.layout(), field, order)
    }

    /// [`Self::load`] with `order`, for a caller that already holds the
    /// instance's `layout`.
    #[must_use]
    #[inline(always)]
    pub fn load_in(self, layout: ClassLayout<'_>, field: usize, order: Ordering) -> Field {
        // SAFETY: the layout places `field` within the cell.
        let bits = unsafe { self.word(layout.slot(field)) }.load(order);
        match layout.kind(field) {
            SlotKind::Int => Field::Value(Value::int(bits.cast_signed())),
            SlotKind::Float => Field::Float(f64::from_bits(bits)),
            SlotKind::Bool => Field::Value(Value::bool(bits != 0)),
            // SAFETY: a `Value` slot holds a live value's bits.
            SlotKind::Value => Field::Value(unsafe { Value::from_bits(bits) }),
        }
    }

    /// `field`, which the class declares as anything but a `float`.
    ///
    /// # Panics
    ///
    /// When `field` is a raw float, which only boxing makes a value.
    #[must_use]
    pub fn load_value(self, field: usize) -> Value {
        match self.load(field) {
            Field::Value(value) => value,
            Field::Float(_) => panic!("field {field} is a raw float"),
        }
    }

    /// `field`, which the class declares as a `float`.
    ///
    /// # Panics
    ///
    /// When `field` is not a raw float.
    #[must_use]
    pub fn load_float(self, field: usize) -> f64 {
        match self.load(field) {
            Field::Float(float) => float,
            Field::Value(value) => panic!("field {field} holds {value:?}, not a raw float"),
        }
    }

    /// Every field, in order.
    pub fn fields(self) -> impl Iterator<Item = Field> {
        (0..self.field_len()).map(move |field| self.load(field))
    }

    /// Store `value` into `field`, ordered for an instance other threads can
    /// reach. A value of the wrong kind for the slot is refused, never
    /// stored.
    #[inline(always)]
    pub fn store(self, field: usize, value: Value) -> Result<(), SlotKindMismatch> {
        self.store_ordered(field, value, Ordering::Release)
    }

    /// [`Self::store`] on an instance no other thread can reach.
    #[inline(always)]
    pub fn store_relaxed(self, field: usize, value: Value) -> Result<(), SlotKindMismatch> {
        self.store_ordered(field, value, Ordering::Relaxed)
    }

    #[inline(always)]
    fn store_ordered(
        self,
        field: usize,
        value: Value,
        order: Ordering,
    ) -> Result<(), SlotKindMismatch> {
        self.store_in(self.layout(), field, value, order)
    }

    /// [`Self::store`] with `order`, for a caller that already holds the
    /// instance's `layout`.
    #[inline(always)]
    pub fn store_in(
        self,
        layout: ClassLayout<'_>,
        field: usize,
        value: Value,
        order: Ordering,
    ) -> Result<(), SlotKindMismatch> {
        let kind = layout.kind(field);
        let bits = slot_bits(kind, value).ok_or(SlotKindMismatch { field, kind, value })?;
        // SAFETY: the layout places `field` within the cell.
        unsafe { self.word(layout.slot(field)) }.store(bits, order);
        Ok(())
    }
}

/// The bits a slot of `kind` holds for `value`, if it is of that kind.
#[inline(always)]
fn slot_bits(kind: SlotKind, value: Value) -> Option<u64> {
    match kind {
        SlotKind::Int => value.as_int().map(i64::cast_unsigned),
        SlotKind::Bool => value.as_bool().map(u64::from),
        SlotKind::Value => Some(value.bits()),
        // SAFETY: a value being stored is live.
        SlotKind::Float => match unsafe { value.as_object_ptr()?.get() } {
            Object::Float(float) => Some(float.to_bits()),
            _ => None,
        },
    }
}
