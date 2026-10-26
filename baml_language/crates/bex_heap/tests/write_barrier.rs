//! Pins what the write barrier does, through `write_barrier` and
//! `conservative_write_barrier` themselves rather than `mark_card_for_ptr`.
//!
//! Minor collections trace Gen0 and Gen1 together, so the only reference the
//! barrier must record is one from a Gen2 object to a younger one. Each case
//! checks the card count, and the old→young cases also check that the young
//! object survives a minor collection where the card is its only root.

use std::sync::Arc;

use bex_heap::{BexHeap, CollectionLevel, Generation, Tlab};
use bex_vm_types::{HeapPtr, Object, Value};

fn alloc_slot_array(tlab: &mut Tlab) -> HeapPtr {
    tlab.alloc_array(baml_type::RealizedTy::unknown(), vec![Value::NULL])
}

/// Promote `ptrs` with one collection at `level`, returning their new addresses.
fn collect(
    heap: &BexHeap,
    tlab: &mut Tlab,
    ptrs: &[HeapPtr],
    level: CollectionLevel,
) -> Vec<HeapPtr> {
    let (_, roots, _) = unsafe { heap.collect_garbage_generational(ptrs, level) };
    tlab.invalidate();
    roots
}

/// Store `value` into slot 0 of `array` and run the barrier, as a mutator does.
fn store(heap: &BexHeap, array: HeapPtr, value: Value) {
    heap.write_barrier(array, value);
    unsafe {
        let Object::Array(data) = array.get_mut() else {
            panic!("expected array")
        };
        data.data_unchecked_mut()[0] = value;
    }
}

/// Slot 0 of `array`, which must be a string; returns its pointer and text.
fn slot_string(array: HeapPtr) -> (HeapPtr, String) {
    let Object::Array(data) = (unsafe { array.get() }) else {
        panic!("expected array")
    };
    let ptr = data
        .get(0)
        .and_then(|value| value.as_object_ptr())
        .expect("slot 0 holds an object");
    let Object::String(text) = (unsafe { ptr.get() }) else {
        panic!("slot 0 is not a string")
    };
    (ptr, text.to_string())
}

#[test]
fn old_container_young_value_is_carded_and_survives() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let array = alloc_slot_array(&mut tlab);
    let old = collect(&heap, &mut tlab, &[array], CollectionLevel::Major)[0];
    assert_eq!(heap.generation_of(old), Generation::Gen2);

    let young = tlab.alloc_string("young".to_string());
    store(&heap, old, Value::object(young));
    assert_eq!(heap.gen2_dirty_card_count(), 1);

    // Only the card keeps `young` alive.
    collect(&heap, &mut tlab, &[old], CollectionLevel::Minor);
    let (moved, text) = slot_string(old);
    assert_eq!(text, "young");
    assert_eq!(heap.generation_of(moved), Generation::Gen1);
}

#[test]
fn old_container_gen1_value_is_carded_and_survives() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let array = alloc_slot_array(&mut tlab);
    let old = collect(&heap, &mut tlab, &[array], CollectionLevel::Major)[0];
    let value = tlab.alloc_string("middle".to_string());
    let roots = collect(&heap, &mut tlab, &[old, value], CollectionLevel::Minor);
    let middle = roots[1];
    assert_eq!(heap.generation_of(middle), Generation::Gen1);

    store(&heap, old, Value::object(middle));
    assert_eq!(heap.gen2_dirty_card_count(), 1);

    // Minor collections trace Gen1 too, so the card is again the only root.
    collect(&heap, &mut tlab, &[old], CollectionLevel::Minor);
    let (moved, text) = slot_string(old);
    assert_eq!(text, "middle");
    assert_eq!(heap.generation_of(moved), Generation::Gen2);
}

#[test]
fn old_container_old_value_is_not_carded() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let array = alloc_slot_array(&mut tlab);
    let value = tlab.alloc_string("old".to_string());
    let roots = collect(&heap, &mut tlab, &[array, value], CollectionLevel::Major);
    assert_eq!(heap.generation_of(roots[1]), Generation::Gen2);

    store(&heap, roots[0], Value::object(roots[1]));
    assert_eq!(heap.gen2_dirty_card_count(), 0);
}

#[test]
fn old_container_non_object_value_is_not_carded() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let array = alloc_slot_array(&mut tlab);
    let old = collect(&heap, &mut tlab, &[array], CollectionLevel::Major)[0];

    store(&heap, old, Value::int(7));
    store(&heap, old, Value::NULL);
    assert_eq!(heap.gen2_dirty_card_count(), 0);
}

#[test]
fn young_containers_are_not_carded() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let array = alloc_slot_array(&mut tlab);
    let middle = collect(&heap, &mut tlab, &[array], CollectionLevel::Minor)[0];
    assert_eq!(heap.generation_of(middle), Generation::Gen1);
    let nursery = alloc_slot_array(&mut tlab);
    assert_eq!(heap.generation_of(nursery), Generation::Gen0);

    let young = tlab.alloc_string("young".to_string());
    store(&heap, nursery, Value::object(young));
    store(&heap, middle, Value::object(young));
    assert_eq!(heap.gen2_dirty_card_count(), 0);

    // Gen1 is traced by a minor collection, so no card is needed for its
    // reference to `young`.
    let middle = collect(&heap, &mut tlab, &[middle], CollectionLevel::Minor)[0];
    assert_eq!(slot_string(middle).1, "young");
}

/// Generation lookup must find containers in every old chunk, not just the first.
#[test]
fn old_containers_in_later_chunks_are_carded() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    // Two chunks plus one; `ChunkedVec` chunks hold 4096 objects.
    let count = 4096 * 2 + 1;
    let arrays: Vec<_> = (0..count).map(|_| alloc_slot_array(&mut tlab)).collect();
    let old = collect(&heap, &mut tlab, &arrays, CollectionLevel::Major);
    let first = old[0];
    let last = old[count - 1];
    assert_eq!(heap.generation_of(first), Generation::Gen2);
    assert_eq!(heap.generation_of(last), Generation::Gen2);

    let young = tlab.alloc_string("far".to_string());
    store(&heap, last, Value::object(young));
    assert_eq!(heap.gen2_dirty_card_count(), 1);
    store(&heap, first, Value::object(young));
    assert_eq!(heap.gen2_dirty_card_count(), 2);

    collect(&heap, &mut tlab, &[first, last], CollectionLevel::Minor);
    assert_eq!(slot_string(last).1, "far");
    assert_eq!(slot_string(first).1, "far");
}

#[test]
fn conservative_barrier_cards_only_gen2_containers() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let array = alloc_slot_array(&mut tlab);
    let old = collect(&heap, &mut tlab, &[array], CollectionLevel::Major)[0];
    let array = alloc_slot_array(&mut tlab);
    let middle = collect(&heap, &mut tlab, &[old, array], CollectionLevel::Minor)[1];
    let nursery = alloc_slot_array(&mut tlab);

    heap.conservative_write_barrier(nursery);
    heap.conservative_write_barrier(middle);
    assert_eq!(heap.gen2_dirty_card_count(), 0);
    heap.conservative_write_barrier(old);
    assert_eq!(heap.gen2_dirty_card_count(), 1);
}
