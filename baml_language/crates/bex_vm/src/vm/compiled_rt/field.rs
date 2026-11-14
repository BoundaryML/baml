//! Field access and construction for compiled code that knows its class.
//!
//! A compiled body that reads `a.x` with `a` statically of class `C` knows
//! where `x` lives and what it holds (`bex_vm_types::ClassLayout`), so the
//! access is a header compare and one load or store at a constant slot, in
//! the slot's own encoding. Any other receiver takes the checked generic path,
//! which reads or writes the field as a `Value` and reports what is wrong.
//! `LOCAL` drops the ordering for an instance escape analysis proved
//! thread-local, as `load_field_local` does.

#![allow(unsafe_code)]
#![allow(
    clippy::inline_always,
    reason = "the fast path is a header compare and one load or store"
)]

use std::sync::atomic::{AtomicU64, Ordering};

use bex_vm_types::{HeapPtr, InstanceRef, Value};

use super::{expect_bool, expect_f64, expect_int};
use crate::{errors::VmError, vm::BexVm};

/// The word `slot` of `instance` when it is an instance of exactly `class`.
#[inline(always)]
fn class_slot(instance: Value, class: HeapPtr, slot: usize) -> Option<&'static AtomicU64> {
    // SAFETY: a value a compiled body holds names a live object.
    let instance = unsafe { InstanceRef::of(instance.as_object_ptr()?) }?;
    // SAFETY: emitted code passes the slot `class`'s layout gives the field,
    // and the header just confirmed the instance is of `class`.
    instance
        .is_of(class)
        .then(|| unsafe { instance.word(slot) })
}

const fn load_order(local: bool) -> Ordering {
    if local {
        Ordering::Relaxed
    } else {
        Ordering::Acquire
    }
}

const fn store_order(local: bool) -> Ordering {
    if local {
        Ordering::Relaxed
    } else {
        Ordering::Release
    }
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
        match class_slot(instance, class, slot) {
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
        if let Some(word) = class_slot(instance, class, slot) {
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

    #[inline(always)]
    pub fn field_f64<const LOCAL: bool>(
        &mut self,
        instance: Value,
        class: HeapPtr,
        field: usize,
        slot: usize,
    ) -> Result<f64, Box<VmError>> {
        self.load_slot::<LOCAL, _>((instance, class, field, slot), f64::from_bits, expect_f64)
    }

    #[inline(always)]
    pub fn field_i64<const LOCAL: bool>(
        &mut self,
        instance: Value,
        class: HeapPtr,
        field: usize,
        slot: usize,
    ) -> Result<i64, Box<VmError>> {
        self.load_slot::<LOCAL, _>((instance, class, field, slot), u64::cast_signed, expect_int)
    }

    #[inline(always)]
    pub fn field_bool<const LOCAL: bool>(
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

    #[inline(always)]
    pub fn field_value<const LOCAL: bool>(
        &mut self,
        instance: Value,
        class: HeapPtr,
        field: usize,
        slot: usize,
    ) -> Result<Value, Box<VmError>> {
        self.load_slot::<LOCAL, _>((instance, class, field, slot), value_from_bits, Ok)
    }

    #[inline(always)]
    pub fn set_field_f64<const LOCAL: bool>(
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

    #[inline(always)]
    pub fn set_field_i64<const LOCAL: bool>(
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

    #[inline(always)]
    pub fn set_field_bool<const LOCAL: bool>(
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

    #[inline(always)]
    pub fn set_field_value<const LOCAL: bool>(
        &mut self,
        instance: Value,
        class: HeapPtr,
        field: usize,
        slot: usize,
        new: Value,
    ) -> Result<(), Box<VmError>> {
        self.store_slot::<LOCAL>((instance, class, field, slot), new.bits(), |_| new)
    }

    /// An instance of the non-generic `class` whose fields, in order, hold
    /// `words`, each already in its slot's encoding.
    #[inline(always)]
    pub fn new_instance_of<const N: usize>(&mut self, class: HeapPtr, words: [u64; N]) -> Value {
        let instance = self.tlab.alloc_instance_zeroed(class, Box::default());
        assert_eq!(instance.layout().words(), N + 1, "fields for {class:?}");
        // The instance is fresh: no other thread can reach it yet.
        for (slot, word) in (1..).zip(words) {
            // SAFETY: the class has `N` fields after its header, as checked.
            unsafe { instance.word(slot) }.store(word, Ordering::Relaxed);
        }
        Value::object(instance.ptr())
    }
}
