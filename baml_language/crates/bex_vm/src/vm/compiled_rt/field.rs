//! Field access and construction for compiled code that knows its class.
//!
//! A compiled body that reads `a.x` with `a` statically of class `C` knows
//! where `x` lives and what it holds (`bex_vm_types::ClassLayout`), so the
//! access is a header compare and one load or store at a constant slot, in
//! the slot's own encoding. Any other receiver takes the checked generic path,
//! which reads or writes the field as a `Value` and reports what is wrong.
//! `LOCAL` drops the ordering for an instance escape analysis proved
//! thread-local, as `load_field_local` does.
//!
//! # Safety
//!
//! The typed accessors and constructors read and write raw words, so they
//! are `unsafe`: a caller passes what `class`'s layout says. For an
//! accessor, `slot` is the word of field `field` of `class`, and the
//! accessor's type is that slot's kind. For a constructor, `class` is a
//! live, non-generic class of exactly `N` fields and `words` holds each in
//! its slot's encoding (a `Value` slot holds a live value's bits);
//! `new_plain_instance` also needs a class without a finalizer. The emitter
//! reads all of it from the layout, and the body table's fingerprint ties
//! the code to that program. Debug builds check the slot.

#![allow(unsafe_code)]
#![allow(
    clippy::inline_always,
    reason = "the fast path is a header compare and one load or store"
)]

use std::sync::atomic::{AtomicU64, Ordering};

use bex_vm_types::{HeapPtr, InstanceRef, Value};

use super::{expect_bool, expect_f64, expect_int};
use crate::{
    errors::VmError,
    vm::{BexVm, load_order, store_order},
};

/// The word `slot`, holding field `field`, of `instance` when it is an
/// instance of exactly `class`.
#[inline(always)]
fn class_slot(
    instance: Value,
    class: HeapPtr,
    field: usize,
    slot: usize,
) -> Option<&'static AtomicU64> {
    // SAFETY: a value a compiled body holds names a live object.
    let instance = unsafe { InstanceRef::of(instance.as_object_ptr()?) }?;
    if !instance.is_of(class) {
        return None;
    }
    debug_assert!(
        field < instance.field_len() && instance.layout().slot(field) == slot,
        "word {slot} does not hold field {field} of {}",
        instance.class_object()
    );
    // SAFETY: emitted code passes the slot `class`'s layout gives the field,
    // and the header just confirmed the instance is of `class`.
    Some(unsafe { instance.word(slot) })
}
/// The value a `Value` slot holds.
#[inline(always)]
fn value_from_bits(bits: u64) -> Value {
    // SAFETY: only a `Value` slot is read through this, and it holds a live
    // value's bits.
    unsafe { Value::from_bits(bits) }
}

impl BexVm {
    /// Field `field` (word `slot`) of `instance`, a `C` where `class` is `C`,
    /// decoded by `from_bits`; the generic path's value goes through
    /// `from_value`.
    #[inline(always)]
    fn load_slot<const LOCAL: bool, T>(
        &mut self,
        (instance, class, field, slot): (Value, HeapPtr, usize, usize),
        from_bits: fn(u64) -> T,
        from_value: fn(Value) -> Result<T, Box<VmError>>,
    ) -> Result<T, Box<VmError>> {
        match class_slot(instance, class, field, slot) {
            Some(word) => Ok(from_bits(word.load(load_order(LOCAL)))),
            None => self.load_slot_slow::<LOCAL, T>(instance, field, from_value),
        }
    }

    /// [`Self::load_slot`] when `instance` is not a `class`: the checked
    /// generic read.
    #[cold]
    #[inline(never)]
    fn load_slot_slow<const LOCAL: bool, T>(
        &mut self,
        instance: Value,
        field: usize,
        from_value: fn(Value) -> Result<T, Box<VmError>>,
    ) -> Result<T, Box<VmError>> {
        let value = self
            .load_instance_field(instance, field, LOCAL)
            .map_err(Box::new)?;
        from_value(value)
    }

    /// Store `bits` into field `field` (word `slot`) of `instance`; the
    /// generic path stores `value`, built only when it is taken.
    #[inline(always)]
    fn store_slot<const LOCAL: bool>(
        &mut self,
        (instance, class, field, slot): (Value, HeapPtr, usize, usize),
        bits: u64,
        value: impl FnOnce(&mut Self) -> Value,
    ) -> Result<(), Box<VmError>> {
        if let Some(word) = class_slot(instance, class, field, slot) {
            word.store(bits, store_order(LOCAL));
            return Ok(());
        }
        self.store_slot_slow::<LOCAL>(instance, field, value)
    }

    /// [`Self::store_slot`] when `instance` is not a `class`: the checked
    /// generic store of the value `value` builds.
    #[cold]
    #[inline(never)]
    fn store_slot_slow<const LOCAL: bool>(
        &mut self,
        instance: Value,
        field: usize,
        value: impl FnOnce(&mut Self) -> Value,
    ) -> Result<(), Box<VmError>> {
        let value = value(self);
        self.store_instance_field(instance, field, value, LOCAL)
            .map_err(Box::new)
    }

    /// # Safety
    ///
    /// As the `compiled_rt::field` module docs say.
    #[inline(always)]
    pub unsafe fn field_f64<const LOCAL: bool>(
        &mut self,
        instance: Value,
        class: HeapPtr,
        field: usize,
        slot: usize,
    ) -> Result<f64, Box<VmError>> {
        self.load_slot::<LOCAL, _>((instance, class, field, slot), f64::from_bits, expect_f64)
    }

    /// # Safety
    ///
    /// As the `compiled_rt::field` module docs say.
    #[inline(always)]
    pub unsafe fn field_i64<const LOCAL: bool>(
        &mut self,
        instance: Value,
        class: HeapPtr,
        field: usize,
        slot: usize,
    ) -> Result<i64, Box<VmError>> {
        self.load_slot::<LOCAL, _>((instance, class, field, slot), u64::cast_signed, expect_int)
    }

    /// # Safety
    ///
    /// As the `compiled_rt::field` module docs say.
    #[inline(always)]
    pub unsafe fn field_bool<const LOCAL: bool>(
        &mut self,
        instance: Value,
        class: HeapPtr,
        field: usize,
        slot: usize,
    ) -> Result<bool, Box<VmError>> {
        self.load_slot::<LOCAL, _>(
            (instance, class, field, slot),
            |bits| bits != 0,
            expect_bool,
        )
    }

    /// # Safety
    ///
    /// As the `compiled_rt::field` module docs say.
    #[inline(always)]
    pub unsafe fn field_value<const LOCAL: bool>(
        &mut self,
        instance: Value,
        class: HeapPtr,
        field: usize,
        slot: usize,
    ) -> Result<Value, Box<VmError>> {
        self.load_slot::<LOCAL, _>((instance, class, field, slot), value_from_bits, Ok)
    }

    /// # Safety
    ///
    /// As the `compiled_rt::field` module docs say.
    #[inline(always)]
    pub unsafe fn set_field_f64<const LOCAL: bool>(
        &mut self,
        instance: Value,
        class: HeapPtr,
        field: usize,
        slot: usize,
        new: f64,
    ) -> Result<(), Box<VmError>> {
        self.store_slot::<LOCAL>((instance, class, field, slot), new.to_bits(), |vm| {
            vm.float_value(new)
        })
    }

    /// # Safety
    ///
    /// As the `compiled_rt::field` module docs say.
    #[inline(always)]
    pub unsafe fn set_field_i64<const LOCAL: bool>(
        &mut self,
        instance: Value,
        class: HeapPtr,
        field: usize,
        slot: usize,
        new: i64,
    ) -> Result<(), Box<VmError>> {
        self.store_slot::<LOCAL>((instance, class, field, slot), new.cast_unsigned(), |_| {
            Value::int(new)
        })
    }

    /// # Safety
    ///
    /// As the `compiled_rt::field` module docs say.
    #[inline(always)]
    pub unsafe fn set_field_bool<const LOCAL: bool>(
        &mut self,
        instance: Value,
        class: HeapPtr,
        field: usize,
        slot: usize,
        new: bool,
    ) -> Result<(), Box<VmError>> {
        self.store_slot::<LOCAL>((instance, class, field, slot), u64::from(new), |_| {
            Value::bool(new)
        })
    }

    /// # Safety
    ///
    /// As the `compiled_rt::field` module docs say.
    #[inline(always)]
    pub unsafe fn set_field_value<const LOCAL: bool>(
        &mut self,
        instance: Value,
        class: HeapPtr,
        field: usize,
        slot: usize,
        new: Value,
    ) -> Result<(), Box<VmError>> {
        self.store_slot::<LOCAL>((instance, class, field, slot), new.bits(), |_| new)
    }

    /// [`Self::new_instance_of`] for a class without a finalizer: one bump
    /// and the header and field stores.
    ///
    /// # Safety
    ///
    /// As the `compiled_rt::field` module docs say.
    #[inline(always)]
    pub unsafe fn new_plain_instance<const N: usize>(
        &mut self,
        class: HeapPtr,
        words: [u64; N],
    ) -> Value {
        // SAFETY: the caller's contract is `alloc_plain_instance`'s.
        let instance = unsafe { self.tlab.alloc_plain_instance(class, words) };
        Value::object(instance)
    }

    /// An instance of the non-generic `class` whose fields, in order, hold
    /// `words`, each already in its slot's encoding.
    ///
    /// # Safety
    ///
    /// As the `compiled_rt::field` module docs say.
    #[inline(always)]
    pub unsafe fn new_instance_of<const N: usize>(
        &mut self,
        class: HeapPtr,
        words: [u64; N],
    ) -> Value {
        let instance = self.tlab.alloc_instance_zeroed(class, Box::default());
        assert_eq!(instance.layout().words(), N + 1, "fields for {class:?}");
        // The instance is fresh: no other thread can reach it yet.
        for (slot, word) in (1..).zip(words) {
            // SAFETY: the class has `N` fields after its header, as checked.
            unsafe { instance.word(slot) }.store(word, Ordering::Relaxed);
        }
        Value::object(self.tlab.instance_ptr(instance))
    }
}
