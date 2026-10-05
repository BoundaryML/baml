//! Measuring the memory a heap object keeps alive outside its slot.
//!
//! Every object occupies one fixed-size slot. What it owns beyond that — an
//! array's elements, a string's text, a map's table — lives in separate
//! allocations the collector never walks. A [`Meter`] adds those up.
//!
//! One description of an object, [`Object::measure`], answers two questions
//! that differ only in how shared storage is counted:
//!
//! - **Charging an allocation** ([`Meter::charge`]): how much did creating this
//!   object add? Every reference to shared storage counts in full. That
//!   overestimates when storage is shared, which only makes collections more
//!   frequent.
//! - **Taking a census** ([`Meter::census`]): how much do the survivors of a
//!   collection hold? Shared storage is counted once across the whole census,
//!   because an overestimate here would inflate the next allocation budget and
//!   make collections rarer.
//!
//! Code that describes an object never learns which question is being asked.

use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicIsize, Ordering},
    },
};

use bex_str::BexStr;
use indexmap::IndexMap;

use crate::{
    Object, RealizedTy,
    types::{
        Class, Enum, Footprint, Function, InterfaceDef, Objects, Package, RuntimeImplRule, Slots,
        TypeAliasDef, TypeValue,
    },
};

/// One allocator's running balance of payload bytes: what it has allocated
/// outside object slots since it last settled, minus what it has released.
///
/// Written by one thread at a time (the allocator that owns it) and read by
/// the same thread, so every access is a plain load or store with no
/// read-modify-write. It is still an atomic so that a holder can be `Sync`
/// and the balance can be reached through a shared reference.
#[derive(Debug, Default)]
pub struct AllocDebt(AtomicIsize);

impl AllocDebt {
    pub const fn new() -> Self {
        Self(AtomicIsize::new(0))
    }

    /// Add `delta` bytes to the balance. Growth is positive; a release is
    /// negative. Saturates rather than wrapping.
    #[inline]
    pub fn add(&self, delta: isize) {
        let balance = self.0.load(Ordering::Relaxed);
        self.0
            .store(balance.saturating_add(delta), Ordering::Relaxed);
    }

    /// Add `bytes` of growth.
    #[inline]
    pub fn grow(&self, bytes: usize) {
        self.add(isize::try_from(bytes).unwrap_or(isize::MAX));
    }

    /// The balance since the last [`Self::take`].
    #[inline]
    pub fn balance(&self) -> isize {
        self.0.load(Ordering::Relaxed)
    }

    /// The balance since the last call, which resets it to zero.
    #[inline]
    pub fn take(&self) -> isize {
        let balance = self.0.load(Ordering::Relaxed);
        self.0.store(0, Ordering::Relaxed);
        balance
    }
}

/// Adds up the bytes heap objects keep alive outside their slots.
pub struct Meter {
    bytes: usize,
    sharing: Sharing,
}

enum Sharing {
    /// Each reference to shared storage counts in full.
    PerReference,
    /// Shared storage counts once; `seen` holds the allocations already counted.
    Once { seen: HashSet<usize> },
}

impl Meter {
    /// A meter for the cost of allocating objects.
    pub fn charge() -> Self {
        Self {
            bytes: 0,
            sharing: Sharing::PerReference,
        }
    }

    /// A meter for the memory held by a set of live objects. Use one meter for
    /// the whole set, so storage shared between its members is counted once.
    pub fn census() -> Self {
        Self {
            bytes: 0,
            sharing: Sharing::Once {
                seen: HashSet::new(),
            },
        }
    }

    /// Everything reported so far.
    pub fn total(&self) -> usize {
        self.bytes
    }

    /// Report memory owned outright.
    pub fn bytes(&mut self, bytes: usize) {
        self.bytes = self.bytes.saturating_add(bytes);
    }

    /// Report a string's text.
    pub fn string(&mut self, string: &BexStr) {
        self.bytes(match self.sharing {
            Sharing::PerReference => string.unshared_heap_bytes(),
            Sharing::Once { .. } => string.shared_heap_bytes(),
        });
    }

    /// Report an allocation that other objects may also hold, along with
    /// whatever `contents` reports for the memory behind it.
    pub fn shared<T: ?Sized>(&mut self, allocation: &Arc<T>, contents: impl FnOnce(&mut Self)) {
        if let Sharing::Once { seen } = &mut self.sharing
            && !seen.insert(Arc::as_ptr(allocation).cast::<()>().addr())
        {
            return;
        }
        // The two reference counts, then the value itself.
        self.bytes(2 * size_of::<usize>() + size_of_val::<T>(allocation));
        contents(self);
    }
}

impl Object {
    /// Report what this object keeps alive outside its slot.
    ///
    /// The description is shallow where depth is bounded by the program rather
    /// than by data: a type is counted as one node, and a declaration by its
    /// top-level tables, not by every name and annotation inside.
    pub fn measure(&mut self, meter: &mut Meter) {
        match self {
            Object::String(string) => meter.string(string),
            Object::Uint8Array(bytes) => meter.bytes(bytes.get_mut().footprint()),
            Object::Array(array) => {
                meter.bytes(size_of::<RealizedTy>());
                meter.bytes(array.data.get_mut().footprint());
            }
            Object::Map(map) => {
                meter.bytes(2 * size_of::<RealizedTy>());
                meter.bytes(map.data.get_mut().footprint());
            }
            Object::Instance(instance) => {
                meter.bytes(instance.fields.footprint());
                meter.bytes(size_of_val(&*instance.class_type_args));
            }
            Object::Closure(closure) => {
                meter.bytes(size_of_val(&*closure.captures));
                meter.bytes(size_of_val(&*closure.captured_type_args));
            }
            Object::BoundMethod(method) => meter.bytes(size_of_val(&*method.type_args)),
            Object::GenericFunction(function) => meter.bytes(size_of_val(&*function.type_args)),
            Object::HostClosure(closure) => {
                meter.shared(&closure.handle, |_| {});
                meter.bytes(2 * size_of::<RealizedTy>());
                meter.bytes(size_of_val(&*closure.params) + closure.params.footprint());
            }
            Object::Bigint(bigint) => meter.shared(bigint, |meter| {
                meter.bytes(usize::try_from(bigint.bits().div_ceil(8)).unwrap_or(usize::MAX));
            }),
            Object::Future(future) => future.measure(meter),
            // The payload reports nothing about itself yet.
            Object::RustData(data) => meter.shared(data, |_| {}),
            Object::Type(_) => meter.bytes(size_of::<TypeValue>()),
            Object::Function(function) => {
                let bytecode = &function.bytecode;
                meter.bytes(size_of::<Function>());
                meter.bytes(bytecode.instructions.footprint());
                meter.bytes(bytecode.constants.footprint());
                meter.bytes(bytecode.resolved_constants.footprint());
                meter.bytes(bytecode.line_table.footprint());
                meter.bytes(bytecode.meta.footprint());
            }
            Object::Class(class) => {
                meter.bytes(size_of::<Class>());
                meter.bytes(class.fields.footprint());
                meter.bytes(index_map_bytes(&class.methods));
            }
            Object::Enum(enm) => {
                meter.bytes(size_of::<Enum>());
                meter.bytes(enm.variants.footprint());
            }
            Object::Interface(interface) => {
                meter.bytes(size_of::<InterfaceDef>());
                meter.bytes(interface.fields.footprint());
                meter.bytes(interface.methods.footprint());
            }
            Object::ImplRule(rule) => {
                meter.bytes(size_of::<RuntimeImplRule>());
                meter.bytes(index_map_bytes(&rule.methods));
            }
            Object::TypeAlias(_) => meter.bytes(size_of::<TypeAliasDef>()),
            Object::Package(package) => {
                meter.bytes(size_of::<Package>());
                meter.bytes(index_map_bytes(&package.edges));
                meter.bytes(index_map_bytes(&package.classes));
                meter.bytes(index_map_bytes(&package.enums));
                meter.bytes(index_map_bytes(&package.interfaces));
                meter.bytes(index_map_bytes(&package.impl_rules));
                meter.bytes(index_map_bytes(&package.type_aliases));
                meter.bytes(index_map_bytes(&package.globals));
                meter.bytes(match &package.slots {
                    Slots::Own { cells, .. } => size_of_val(&**cells),
                    Slots::Program { .. } => 0,
                });
                meter.bytes(match &package.objects {
                    Objects::Own(objects) => size_of_val(&**objects),
                    Objects::Program => 0,
                });
            }
            Object::Cell(_) | Object::Variant(_) | Object::Float(_) | Object::Tombstone => {}
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => {}
        }
    }
}

/// The entry storage and index of an `IndexMap`, not what its keys and values
/// point at.
fn index_map_bytes<K, V>(map: &IndexMap<K, V>) -> usize {
    // Per entry: the key, the value, the stored hash, and an index slot.
    map.capacity()
        .saturating_mul(size_of::<K>() + size_of::<V>() + 2 * size_of::<usize>())
}

#[cfg(test)]
mod tests {
    use bex_str::BexStr;

    use super::*;
    use crate::{
        MapData, Value,
        types::{Array, Instance, Map},
    };

    fn charged(mut object: Object) -> usize {
        let mut meter = Meter::charge();
        object.measure(&mut meter);
        meter.total()
    }

    fn census(objects: &mut [Object]) -> usize {
        let mut meter = Meter::census();
        for object in objects {
            object.measure(&mut meter);
        }
        meter.total()
    }

    #[test]
    fn objects_with_nothing_outside_their_slot_measure_zero() {
        assert_eq!(charged(Object::Float(1.5)), 0);
        assert_eq!(charged(Object::Tombstone), 0);
        assert_eq!(charged(Object::String(BexStr::from("inline"))), 0);
    }

    #[test]
    fn containers_are_measured_by_capacity_not_length() {
        let mut elements = Vec::with_capacity(1_000);
        elements.push(Value::int(1));
        let array = Object::Array(Array::new(RealizedTy::int(), elements));
        assert_eq!(
            charged(array),
            size_of::<RealizedTy>() + 1_000 * size_of::<Value>()
        );

        let mut bytes = Vec::with_capacity(4_096);
        bytes.push(7u8);
        assert_eq!(charged(Object::Uint8Array(bytes.into())), 4_096);
    }

    #[test]
    fn an_instance_is_measured_by_its_fields() {
        let instance = Instance::new(
            crate::HeapPtr::null(),
            Box::new([]),
            vec![Value::int(1), Value::int(2), Value::int(3)],
        );
        assert_eq!(charged(Object::Instance(instance)), 3 * size_of::<Value>());
    }

    #[test]
    fn a_map_grows_with_its_entries() {
        let empty = charged(Object::Map(Map::new(
            RealizedTy::string(),
            RealizedTy::int(),
            MapData::new(),
        )));
        let entries = 1_000;
        let full = charged(Object::Map(Map::new(
            RealizedTy::string(),
            RealizedTy::int(),
            MapData::from_hashed_entries((0..entries).map(|i| (i, Value::int(1), Value::int(2)))),
        )));
        // Each entry stores at least an id, a key, a value and a hash.
        let per_entry = usize::try_from(entries).unwrap() * 4 * size_of::<u64>();
        assert!(
            full >= empty + per_entry,
            "{full} bytes for {entries} entries, {empty} when empty"
        );
    }

    #[test]
    fn charging_counts_shared_text_per_reference_and_a_census_counts_it_once() {
        let text = BexStr::from("x".repeat(100_000));
        let whole = text.unshared_heap_bytes();
        let mut objects = [
            Object::String(text.clone()),
            Object::String(text.clone()),
            Object::String(text),
        ];

        let per_reference: usize = objects.iter().cloned().map(charged).sum();
        assert_eq!(per_reference, 3 * whole);

        let once = census(&mut objects);
        assert!(once >= whole);
        assert!(once < whole + objects.len());
    }

    #[test]
    fn charging_counts_a_shared_allocation_per_reference_and_a_census_counts_it_once() {
        let bigint = Arc::new(num_bigint::BigInt::from(1u8) << 80_000);
        let one = charged(Object::Bigint(Arc::clone(&bigint)));
        assert!(one >= 10_000, "{one} bytes for a 10 KB integer");

        let mut objects = [
            Object::Bigint(Arc::clone(&bigint)),
            Object::Bigint(Arc::clone(&bigint)),
        ];
        assert_eq!(objects.iter().cloned().map(charged).sum::<usize>(), 2 * one);
        assert_eq!(census(&mut objects), one);
    }

    #[test]
    fn an_opaque_payload_is_measured_as_its_allocation() {
        let payload: Arc<dyn std::any::Any + Send + Sync> = Arc::new([0u64; 16]);
        assert_eq!(
            charged(Object::RustData(payload)),
            2 * size_of::<usize>() + 16 * size_of::<u64>()
        );
    }
}
