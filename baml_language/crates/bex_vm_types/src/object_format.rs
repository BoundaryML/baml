//! The shared object format (`documents/runtime-v2-shared-design.md`, section 1):
//! the header every heap object starts with, the fixed descriptor IDs, and
//! reading and writing the three slot kinds.
//!
//! Replaces `bex_heap::descriptor::{Header, TaggedSlot, TaggedKind}`, which
//! move here so a [`HeapPtr`] can read its object's header without the heap,
//! and the `AtomicValueSlot` field storage of the old `Instance`.
//!
//! # Pointers
//!
//! A [`HeapPtr`] (and the pointer bits of a `Value` or a pointer slot) names
//! the byte just after the object's 8-byte [`Header`]. The `#[repr(C)]`
//! structs in this crate are declared from the header, so compiled code casts
//! `ptr - HEADER_BYTES`. Compile-time objects carry a header too
//! (`bex_heap`'s compile-time cells), so every pointer has one.
//!
//! # Slots
//!
//! | Kind | Bytes | Shared read | Shared write |
//! |---|---:|---|---|
//! | raw `int`/`float`/`bool` | 8 | Relaxed load | Relaxed store |
//! | pointer | 8 | Acquire load | Release store |
//! | tagged ([`TaggedSlot`]) | 16 | take the tag, read, put it back | take the tag, write, publish |
//!
//! A thread-local access (escape analysis proved no other thread can reach
//! the object) uses Relaxed loads and stores and never takes a tag.

#![allow(unsafe_code)]

use std::sync::atomic::{AtomicU8, AtomicU16, AtomicU64, Ordering};

use crate::{
    HeapPtr, Object, Value,
    layout::{DescriptorId, ElemKind, RawKind, SlotKind},
};

/// Bytes of the [`Header`] before every object.
pub const HEADER_BYTES: usize = 8;

/// The first eight bytes of every heap object.
#[repr(C)]
#[derive(Debug)]
pub struct Header {
    pub descriptor: DescriptorId,
    /// [`Header::CLEANED`]; the rest are reserved (finalizer registered,
    /// pinned).
    flags: AtomicU16,
    /// Cell size in 16-byte granules for a small cell; zero for a large one
    /// and for a compile-time object.
    granules: u16,
}

const _: () = assert!(size_of::<Header>() == HEADER_BYTES);

impl Header {
    /// BEP-042: the instance's `cleanup` has started.
    pub const CLEANED: u16 = 1;

    pub const fn new(descriptor: DescriptorId, granules: u16) -> Self {
        Self {
            descriptor,
            flags: AtomicU16::new(0),
            granules,
        }
    }

    #[inline]
    pub fn granules(&self) -> u16 {
        self.granules
    }

    /// Set `flag`; whether it was already set.
    #[inline]
    pub fn test_and_set(&self, flag: u16) -> bool {
        self.flags.fetch_or(flag, Ordering::AcqRel) & flag != 0
    }

    #[inline]
    pub fn is_set(&self, flag: u16) -> bool {
        self.flags.load(Ordering::Acquire) & flag != 0
    }
}

/// The descriptor of every VM `Object` enum cell, compile-time objects
/// included.
pub const OBJECT_DESC: DescriptorId = DescriptorId(0);

/// The first descriptor ID handed to a class instance layout. IDs below it
/// are fixed: [`OBJECT_DESC`] and the list, backing and map descriptors.
pub const FIRST_INSTANCE_DESC: DescriptorId = DescriptorId(64);

/// What an object is, decided from its descriptor ID alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// A VM `Object` enum.
    Object,
    /// A class instance laid out by its class's
    /// [`InstanceLayout`](crate::types::InstanceLayout).
    Instance,
}

impl Format {
    #[inline]
    pub fn of(descriptor: DescriptorId) -> Self {
        if descriptor == OBJECT_DESC {
            Format::Object
        } else {
            debug_assert!(
                descriptor.0 >= FIRST_INSTANCE_DESC.0,
                "descriptor {descriptor:?} names no object format"
            );
            Format::Instance
        }
    }
}

impl HeapPtr {
    /// The header before the object.
    ///
    /// # Safety
    /// The pointer names a live object.
    #[inline]
    pub unsafe fn header(self) -> &'static Header {
        // SAFETY: every object, compile-time ones included, has its header
        // in the 8 bytes before it.
        unsafe {
            &*self
                .as_ptr()
                .cast::<u8>()
                .sub(HEADER_BYTES)
                .cast::<Header>()
        }
    }

    /// The object's format.
    ///
    /// # Safety
    /// The pointer names a live object.
    #[inline]
    pub unsafe fn format(self) -> Format {
        // SAFETY: forwarded to the caller.
        Format::of(unsafe { self.header() }.descriptor)
    }
}

/// How a slot access synchronizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Other threads may reach the object.
    Shared,
    /// Escape analysis proved no other thread can reach it.
    ThreadLocal,
}

/// What a slot holds, as the interpreter reads it: a `Value`, except that a
/// float is unboxed. A float leaving a slot for a local is boxed by the
/// caller, which has the allocator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SlotValue {
    Value(Value),
    Float(f64),
}

impl SlotValue {
    /// The value, boxing a float with `box_float`.
    #[inline]
    pub fn into_value(self, box_float: impl FnOnce(f64) -> Value) -> Value {
        match self {
            SlotValue::Value(value) => value,
            SlotValue::Float(float) => box_float(float),
        }
    }

    /// The value of a slot known not to hold a float.
    ///
    /// # Panics
    /// If it holds a float.
    #[track_caller]
    pub fn expect_value(self) -> Value {
        match self {
            SlotValue::Value(value) => value,
            SlotValue::Float(float) => panic!("expected a non-float slot, found {float}"),
        }
    }

    pub fn as_float(self) -> Option<f64> {
        match self {
            SlotValue::Float(float) => Some(float),
            SlotValue::Value(_) => None,
        }
    }
}

/// A value that does not fit the slot it was stored to: the declared type
/// and the stored value disagree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a {slot:?} slot cannot hold the value {value:?}")]
pub struct SlotMismatch {
    pub slot: SlotKind,
    pub value: Value,
}

/// The tag of a [`TaggedSlot`].
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaggedKind {
    /// A reader or writer holds the slot.
    Locked = 0,
    Ptr = 1,
    Int = 2,
    Float = 3,
    Bool = 4,
    Null = 5,
}

impl TaggedKind {
    fn from_u8(tag: u8) -> Self {
        match tag {
            1 => TaggedKind::Ptr,
            2 => TaggedKind::Int,
            3 => TaggedKind::Float,
            4 => TaggedKind::Bool,
            5 => TaggedKind::Null,
            _ => TaggedKind::Locked,
        }
    }
}

/// A 16-byte slot that may hold a pointer or a scalar: `SafeValue` in the
/// design. The tag doubles as a spinlock: an access swaps in
/// [`TaggedKind::Locked`], reads or writes the payload, and puts a tag back.
#[repr(C)]
#[derive(Debug)]
pub struct TaggedSlot {
    kind: AtomicU8,
    _pad: [u8; 7],
    payload: AtomicU64,
}

const _: () = assert!(size_of::<TaggedSlot>() == 16);

impl TaggedSlot {
    /// The tag and payload, read by the collector while no mutator runs.
    #[inline]
    pub fn read_paused(&self) -> (TaggedKind, u64) {
        (
            TaggedKind::from_u8(self.kind.load(Ordering::Relaxed)),
            self.payload.load(Ordering::Relaxed),
        )
    }

    /// Take the slot, returning the tag it held.
    fn lock(&self) -> TaggedKind {
        loop {
            let tag = self.kind.swap(TaggedKind::Locked as u8, Ordering::Acquire);
            if tag != TaggedKind::Locked as u8 {
                return TaggedKind::from_u8(tag);
            }
            std::hint::spin_loop();
        }
    }

    fn read(&self, access: Access) -> (TaggedKind, u64) {
        match access {
            Access::ThreadLocal => self.read_paused(),
            Access::Shared => {
                let tag = self.lock();
                let payload = self.payload.load(Ordering::Relaxed);
                self.kind.store(tag as u8, Ordering::Release);
                (tag, payload)
            }
        }
    }

    fn write(&self, tag: TaggedKind, payload: u64, access: Access) {
        if access == Access::Shared {
            self.lock();
        }
        self.payload.store(payload, Ordering::Relaxed);
        self.kind.store(tag as u8, Ordering::Release);
    }
}

/// The float a `Value` boxes, if it is a float.
#[inline]
fn boxed_float(value: Value) -> Option<f64> {
    let ptr = value.as_object_ptr()?;
    // SAFETY: a `Value` names a live object.
    match unsafe { ptr.get() } {
        Object::Float(float) => Some(*float),
        _ => None,
    }
}

/// `ptr` as a `Value`, null for zero.
#[inline]
fn pointer_value(bits: u64) -> Value {
    // SAFETY: a pointer slot holds zero or the bits of a live object's `Value`.
    unsafe { Value::from_bits(bits) }
}

/// A raw word read back as a `Value`.
#[inline]
fn raw_value(raw: RawKind, bits: u64) -> SlotValue {
    match raw {
        #[expect(
            clippy::cast_possible_wrap,
            reason = "a raw slot stores the i64's bits"
        )]
        RawKind::Int => SlotValue::Value(Value::int(bits as i64)),
        RawKind::Float => SlotValue::Float(f64::from_bits(bits)),
        RawKind::Bool => SlotValue::Value(Value::bool(bits != 0)),
    }
}

/// The raw word for `value` in a slot holding `raw`. A null stores zero: an
/// instance's fields are null until its constructor stores them.
fn raw_bits(raw: RawKind, value: Value) -> Option<u64> {
    if value.is_null() {
        return Some(0);
    }
    match raw {
        #[expect(clippy::cast_sign_loss, reason = "a raw slot stores the i64's bits")]
        RawKind::Int => value.as_int().map(|int| int as u64),
        RawKind::Float => boxed_float(value).map(f64::to_bits),
        RawKind::Bool => value.as_bool().map(u64::from),
    }
}

/// The tag and payload for `value` in a tagged slot.
fn tagged_parts(value: Value) -> Option<(TaggedKind, u64)> {
    if value.is_null() {
        return Some((TaggedKind::Null, 0));
    }
    if let Some(int) = value.as_int() {
        #[expect(clippy::cast_sign_loss, reason = "the payload stores the i64's bits")]
        return Some((TaggedKind::Int, int as u64));
    }
    if let Some(boolean) = value.as_bool() {
        return Some((TaggedKind::Bool, u64::from(boolean)));
    }
    if let Some(float) = boxed_float(value) {
        return Some((TaggedKind::Float, float.to_bits()));
    }
    value.is_object().then_some((TaggedKind::Ptr, value.bits()))
}

fn tagged_value(tag: TaggedKind, payload: u64) -> SlotValue {
    match tag {
        TaggedKind::Ptr => SlotValue::Value(pointer_value(payload)),
        #[expect(
            clippy::cast_possible_wrap,
            reason = "the payload stores the i64's bits"
        )]
        TaggedKind::Int => SlotValue::Value(Value::int(payload as i64)),
        TaggedKind::Float => SlotValue::Float(f64::from_bits(payload)),
        TaggedKind::Bool => SlotValue::Value(Value::bool(payload != 0)),
        TaggedKind::Null => SlotValue::Value(Value::NULL),
        TaggedKind::Locked => unreachable!("a tagged slot read returned its lock"),
    }
}

/// Read the slot of kind `kind` at `addr`.
///
/// # Safety
/// `addr` addresses a slot of `kind` inside a live object.
#[inline]
pub unsafe fn read_slot(addr: *const u8, kind: SlotKind, access: Access) -> SlotValue {
    match kind {
        SlotKind::Tagged16 => {
            // SAFETY: forwarded to the caller.
            let slot = unsafe { &*addr.cast::<TaggedSlot>() };
            let (tag, payload) = slot.read(access);
            tagged_value(tag, payload)
        }
        SlotKind::Raw(raw) => {
            // SAFETY: forwarded to the caller.
            let word = unsafe { &*addr.cast::<AtomicU64>() };
            raw_value(raw, word.load(Ordering::Relaxed))
        }
        SlotKind::Ptr => {
            // SAFETY: forwarded to the caller.
            let word = unsafe { &*addr.cast::<AtomicU64>() };
            let order = match access {
                Access::Shared => Ordering::Acquire,
                Access::ThreadLocal => Ordering::Relaxed,
            };
            SlotValue::Value(pointer_value(word.load(order)))
        }
    }
}

/// Write `value` to the slot of kind `kind` at `addr`.
///
/// # Safety
/// `addr` addresses a slot of `kind` inside a live object.
#[inline]
pub unsafe fn write_slot(
    addr: *const u8,
    kind: SlotKind,
    value: Value,
    access: Access,
) -> Result<(), SlotMismatch> {
    let mismatch = SlotMismatch { slot: kind, value };
    match kind {
        SlotKind::Tagged16 => {
            let (tag, payload) = tagged_parts(value).ok_or(mismatch)?;
            // SAFETY: forwarded to the caller.
            unsafe { &*addr.cast::<TaggedSlot>() }.write(tag, payload, access);
        }
        SlotKind::Raw(raw) => {
            let bits = raw_bits(raw, value).ok_or(mismatch)?;
            // SAFETY: forwarded to the caller.
            unsafe { &*addr.cast::<AtomicU64>() }.store(bits, Ordering::Relaxed);
        }
        SlotKind::Ptr => {
            if !(value.is_null() || value.is_object()) {
                return Err(mismatch);
            }
            let order = match access {
                Access::Shared => Ordering::Release,
                Access::ThreadLocal => Ordering::Relaxed,
            };
            // SAFETY: forwarded to the caller.
            unsafe { &*addr.cast::<AtomicU64>() }.store(value.bits(), order);
        }
    }
    Ok(())
}

/// Write a float to a slot of kind `kind`, unboxed where the slot allows.
///
/// # Safety
/// As for [`write_slot`].
pub unsafe fn write_float_slot(
    addr: *const u8,
    kind: SlotKind,
    float: f64,
    access: Access,
) -> Result<(), SlotMismatch> {
    match kind {
        SlotKind::Raw(RawKind::Float) => {
            // SAFETY: forwarded to the caller.
            unsafe { &*addr.cast::<AtomicU64>() }.store(float.to_bits(), Ordering::Relaxed);
            Ok(())
        }
        SlotKind::Tagged16 => {
            // SAFETY: forwarded to the caller.
            unsafe { &*addr.cast::<TaggedSlot>() }.write(
                TaggedKind::Float,
                float.to_bits(),
                access,
            );
            Ok(())
        }
        SlotKind::Raw(_) | SlotKind::Ptr => Err(SlotMismatch {
            slot: kind,
            value: Value::NULL,
        }),
    }
}

/// Copy the slot of kind `kind` at `from` to `to`, keeping a float unboxed.
///
/// # Safety
/// Both address slots of `kind` inside live objects.
pub unsafe fn copy_slot(from: *const u8, to: *const u8, kind: SlotKind, access: Access) {
    // SAFETY: forwarded to the caller.
    let result = match unsafe { read_slot(from, kind, access) } {
        // SAFETY: forwarded to the caller.
        SlotValue::Value(value) => unsafe { write_slot(to, kind, value, access) },
        // SAFETY: forwarded to the caller.
        SlotValue::Float(float) => unsafe { write_float_slot(to, kind, float, access) },
    };
    result.expect("a slot copied to a slot of its own kind fits");
}

/// The pointer a slot of `kind` at `addr` holds, if any, for the collector.
///
/// # Safety
/// `addr` addresses a slot of `kind`; no mutator runs.
pub unsafe fn slot_pointer(addr: *const u8, kind: ElemKind) -> Option<u64> {
    let bits = match kind {
        ElemKind::Raw8 => return None,
        // SAFETY: forwarded to the caller.
        ElemKind::Ptr => unsafe { &*addr.cast::<AtomicU64>() }.load(Ordering::Relaxed),
        ElemKind::Tagged16 => {
            // SAFETY: forwarded to the caller.
            let (tag, payload) = unsafe { &*addr.cast::<TaggedSlot>() }.read_paused();
            if tag != TaggedKind::Ptr {
                return None;
            }
            payload
        }
    };
    (bits != 0).then_some(bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(C, align(16))]
    struct Slots([u8; 32]);

    fn round_trip(kind: SlotKind, value: Value) -> SlotValue {
        let slots = Slots([0xA5; 32]);
        let addr = slots.0.as_ptr();
        for access in [Access::Shared, Access::ThreadLocal] {
            // SAFETY: `slots` is 16-aligned and big enough for any slot.
            unsafe { write_slot(addr, kind, value, access) }.unwrap();
        }
        // SAFETY: as above.
        unsafe { read_slot(addr, kind, Access::Shared) }
    }

    #[test]
    fn scalars_round_trip_through_their_slots() {
        let cases = [
            (SlotKind::Raw(RawKind::Int), Value::int(-42)),
            (SlotKind::Raw(RawKind::Bool), Value::TRUE),
            (SlotKind::Tagged16, Value::int(7)),
            (SlotKind::Tagged16, Value::FALSE),
            (SlotKind::Tagged16, Value::NULL),
            (SlotKind::Ptr, Value::NULL),
        ];
        for (kind, value) in cases {
            assert_eq!(round_trip(kind, value), SlotValue::Value(value), "{kind:?}");
        }
    }

    #[test]
    fn a_null_in_a_raw_slot_reads_back_as_zero() {
        assert_eq!(
            round_trip(SlotKind::Raw(RawKind::Int), Value::NULL),
            SlotValue::Value(Value::int(0))
        );
    }

    #[test]
    fn floats_are_stored_unboxed() {
        let slots = Slots([0; 32]);
        for kind in [SlotKind::Raw(RawKind::Float), SlotKind::Tagged16] {
            // A zeroed tagged slot reads as held; allocation writes null first.
            // SAFETY: as in `round_trip`.
            unsafe { write_slot(slots.0.as_ptr(), kind, Value::NULL, Access::ThreadLocal) }
                .unwrap();
            // SAFETY: as in `round_trip`.
            unsafe { write_float_slot(slots.0.as_ptr(), kind, 2.5, Access::Shared) }.unwrap();
            // SAFETY: as above.
            let read = unsafe { read_slot(slots.0.as_ptr(), kind, Access::Shared) };
            assert_eq!(read, SlotValue::Float(2.5));
        }
    }

    #[test]
    fn a_scalar_does_not_fit_a_pointer_slot() {
        let slots = Slots([0; 32]);
        // SAFETY: as in `round_trip`.
        let stored = unsafe {
            write_slot(
                slots.0.as_ptr(),
                SlotKind::Ptr,
                Value::int(1),
                Access::Shared,
            )
        };
        assert!(stored.is_err());
    }
}
