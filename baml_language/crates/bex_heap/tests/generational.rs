//! Tests the generational promotion ladder (Gen0 → Gen1 → Gen2), GcStats
//! for each collection level, card table integration with minor GC, and
//! multi-cycle behavior.
//!
//! These tests use `collect_garbage_generational` directly and are placed in
//! the `tests/` directory so they can exercise cross-module public APIs.

use std::sync::Arc;

use bex_external_types::WeakHeapRef;
use bex_heap::{BexHeap, CollectionLevel, Generation, Tlab};
use bex_vm_types::Object;

// ============================================================================
// Generation Classification & Promotion Tests
// ============================================================================

#[test]
fn test_minor_gc_promotes_gen0_to_gen1() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let ptr = tlab.alloc_string("promoted".to_string());
    assert_eq!(heap.generation_of(ptr), Generation::Gen0);

    let roots = vec![ptr];
    let (stats, new_roots, _) =
        unsafe { heap.collect_garbage_generational(&roots, CollectionLevel::Minor) };
    tlab.invalidate();

    assert_eq!(stats.level, CollectionLevel::Minor);
    assert_eq!(stats.promoted_to_gen1, 1);
    assert_eq!(stats.promoted_to_gen2, 0);
    assert_eq!(heap.generation_of(new_roots[0]), Generation::Gen1);
}

#[test]
fn test_two_minor_gcs_promote_gen0_to_gen1_to_gen2() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let ptr = tlab.alloc_string("long-lived".to_string());
    assert_eq!(heap.generation_of(ptr), Generation::Gen0);

    // First minor GC: Gen0 → Gen1
    let (_, roots1, _) =
        unsafe { heap.collect_garbage_generational(&[ptr], CollectionLevel::Minor) };
    tlab.invalidate();
    assert_eq!(heap.generation_of(roots1[0]), Generation::Gen1);

    // Allocate new Gen0 objects
    let new_obj = tlab.alloc_string("new".to_string());
    assert_eq!(heap.generation_of(new_obj), Generation::Gen0);

    // Second minor GC: Gen1 → Gen2, Gen0 → Gen1
    let (stats, roots2, _) =
        unsafe { heap.collect_garbage_generational(&[roots1[0], new_obj], CollectionLevel::Minor) };
    tlab.invalidate();

    assert_eq!(stats.promoted_to_gen2, 1); // old Gen1 → Gen2
    assert_eq!(stats.promoted_to_gen1, 1); // new Gen0 → Gen1
    assert_eq!(heap.generation_of(roots2[0]), Generation::Gen2);
    assert_eq!(heap.generation_of(roots2[1]), Generation::Gen1);
}

#[test]
fn test_major_gc_flattens_all_to_gen2() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let a = tlab.alloc_string("a".to_string());
    let b = tlab.alloc_string("b".to_string());

    // Minor GC: both to Gen1
    let (_, roots1, _) =
        unsafe { heap.collect_garbage_generational(&[a, b], CollectionLevel::Minor) };
    tlab.invalidate();

    // Allocate more in Gen0
    let c = tlab.alloc_string("c".to_string());

    // Major GC: everything to Gen2
    let (stats, roots2, _) = unsafe {
        heap.collect_garbage_generational(&[roots1[0], roots1[1], c], CollectionLevel::Major)
    };
    tlab.invalidate();

    assert_eq!(stats.level, CollectionLevel::Major);
    assert_eq!(stats.live_count, 3);
    for &r in &roots2 {
        assert_eq!(heap.generation_of(r), Generation::Gen2);
    }
}

#[test]
fn test_gen2_objects_survive_minor_gc_untouched() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let ptr = tlab.alloc_string("old".to_string());

    // Major GC to promote to Gen2
    let (_, roots, _) =
        unsafe { heap.collect_garbage_generational(&[ptr], CollectionLevel::Major) };
    tlab.invalidate();
    let gen2_ptr = roots[0];
    assert_eq!(heap.generation_of(gen2_ptr), Generation::Gen2);

    // Allocate new Gen0 garbage (not rooted)
    let _garbage = tlab.alloc_string("garbage".to_string());

    // Minor GC: Gen2 object untouched, Gen0 garbage collected
    let (stats, new_roots, fwd) =
        unsafe { heap.collect_garbage_generational(&[gen2_ptr], CollectionLevel::Minor) };
    tlab.invalidate();

    // Gen2 pointer should be identity-mapped (not moved)
    assert_eq!(new_roots[0], gen2_ptr);
    // Identity-mapped Gen2 objects appear in the forwarding map
    assert_eq!(*fwd.get(&gen2_ptr).unwrap(), gen2_ptr);
    // At least the garbage was collected (plus unused TLAB slots)
    assert!(stats.collected_count >= 1);
}

#[test]
fn test_minor_gc_with_empty_gen1() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    // First ever minor GC — Gen1 is empty
    let a = tlab.alloc_string("first".to_string());
    let (stats, new_roots, _) =
        unsafe { heap.collect_garbage_generational(&[a], CollectionLevel::Minor) };
    tlab.invalidate();

    assert_eq!(stats.promoted_to_gen1, 1);
    assert_eq!(stats.promoted_to_gen2, 0);
    assert_eq!(heap.generation_of(new_roots[0]), Generation::Gen1);
}

#[test]
fn test_minor_gc_with_empty_gen0_promotes_gen1_to_gen2() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let a = tlab.alloc_string("will_promote".to_string());

    // Minor GC: Gen0 → Gen1
    let (_, roots1, _) = unsafe { heap.collect_garbage_generational(&[a], CollectionLevel::Minor) };
    tlab.invalidate();

    // No new allocations in Gen0. Minor GC again.
    let (stats, roots2, _) =
        unsafe { heap.collect_garbage_generational(&[roots1[0]], CollectionLevel::Minor) };
    tlab.invalidate();

    // Gen1 → Gen2, no Gen0 objects
    assert_eq!(stats.promoted_to_gen2, 1);
    assert_eq!(stats.promoted_to_gen1, 0);
    assert_eq!(heap.generation_of(roots2[0]), Generation::Gen2);
}

#[test]
fn test_gc_back_to_back_no_allocations_between() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let a = tlab.alloc_string("survivor".to_string());
    let (_, roots1, _) = unsafe { heap.collect_garbage(&[a]) };
    tlab.invalidate();

    // Second GC with no new allocations
    let (stats, roots2, _) = unsafe { heap.collect_garbage(&[roots1[0]]) };
    tlab.invalidate();

    assert_eq!(stats.live_count, 1);
    assert_eq!(stats.collected_count, 0);
    // Data still correct
    let Object::String(s) = (unsafe { roots2[0].get() }) else {
        panic!("not string")
    };
    assert_eq!(s, "survivor");
}

#[test]
fn test_all_garbage_all_generations_empty() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    // Allocate objects, don't root any
    for i in 0..100 {
        tlab.alloc_string(format!("garbage_{i}"));
    }

    let (stats, new_roots, _) = unsafe { heap.collect_garbage(&[]) };
    tlab.invalidate();

    assert_eq!(stats.live_count, 0);
    // TLAB pre-allocates a full chunk (1024 slots), so collected_count >= 100
    assert!(stats.collected_count >= 100);
    assert!(new_roots.is_empty());

    // Verify can allocate again from clean state
    let fresh = tlab.alloc_string("fresh".to_string());
    assert_eq!(heap.generation_of(fresh), Generation::Gen0);
}

// ============================================================================
// GcStats Pin Tests
// ============================================================================

#[test]
fn test_stats_major_all_promoted_to_gen2() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let ptrs: Vec<_> = (0..5).map(|i| tlab.alloc_string(format!("s{i}"))).collect();
    let (stats, _, _) = unsafe { heap.collect_garbage_generational(&ptrs, CollectionLevel::Major) };

    assert_eq!(stats.level, CollectionLevel::Major);
    assert_eq!(stats.live_count, 5);
    // TLAB pre-allocates chunk slots; collected_count >= 0
    assert_eq!(stats.promoted_to_gen2, 5);
    assert_eq!(stats.promoted_to_gen1, 0);
}

#[test]
fn test_stats_minor_partial_survival() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let live = tlab.alloc_string("live".to_string());
    let _dead1 = tlab.alloc_string("dead1".to_string());
    let _dead2 = tlab.alloc_string("dead2".to_string());

    let (stats, _, _) =
        unsafe { heap.collect_garbage_generational(&[live], CollectionLevel::Minor) };

    assert_eq!(stats.level, CollectionLevel::Minor);
    assert_eq!(stats.live_count, 1);
    // TLAB pre-allocates a full chunk (1024 slots); collected_count >= 2
    assert!(stats.collected_count >= 2);
    assert_eq!(stats.promoted_to_gen1, 1);
}

#[test]
fn test_stats_compile_time_not_counted() {
    let compile_time = vec![Object::String("builtin".to_string())];
    let heap = BexHeap::new(compile_time);
    let ct_ptr = heap.compile_time_ptr(0);

    let mut tlab = Tlab::new(Arc::clone(&heap));
    let _garbage = tlab.alloc_string("garbage".to_string());

    // Root only compile-time ptr
    let (stats, _, _) = unsafe { heap.collect_garbage(&[ct_ptr]) };

    // Compile-time object not counted in live_count or collected_count
    // (only runtime objects are tracked in those stats)
    assert_eq!(stats.live_count, 0);
    // At least 1 garbage runtime object collected (plus TLAB chunk padding)
    assert!(stats.collected_count >= 1);
}

// ============================================================================
// Multi-Cycle Accumulation Tests
// ============================================================================

#[test]
fn test_repeated_minor_gcs_accumulate_gen2() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let mut persistent_roots = vec![];

    for cycle in 0..5 {
        // Allocate one persistent object per cycle
        let persistent = tlab.alloc_string(format!("persistent_{cycle}"));
        // Allocate some garbage
        for i in 0..10 {
            tlab.alloc_string(format!("garbage_{cycle}_{i}"));
        }

        persistent_roots.push(persistent);

        // Minor GC
        let (stats, new_roots, _) =
            unsafe { heap.collect_garbage_generational(&persistent_roots, CollectionLevel::Minor) };
        tlab.invalidate();

        // Update roots from forwarding
        persistent_roots = new_roots;

        // Minor GC live_count = new_gen1_count + promoted_to_gen2.
        // This does NOT include existing Gen2 objects that are identity-mapped.
        // At cycle 0: 1 Gen0 → Gen1: live=1
        // At cycle 1: 1 Gen1 → Gen2 + 1 Gen0 → Gen1: live=2
        // At cycle 2+: 1 Gen1 → Gen2 + 1 Gen0 → Gen1: live=2 (Gen2 objects not counted)
        let expected_live = if cycle == 0 { 1 } else { 2 };
        assert_eq!(
            stats.live_count, expected_live,
            "cycle {cycle}: minor GC live_count (processed objects) should be {expected_live}"
        );

        // Also verify data integrity for each object
        for (i, root) in persistent_roots.iter().enumerate() {
            let Object::String(s) = (unsafe { root.get() }) else {
                panic!("cycle {cycle}: root {i} not a string")
            };
            assert_eq!(
                s,
                &format!("persistent_{i}"),
                "cycle {cycle}: root {i} data wrong"
            );
        }
    }

    // After 5 cycles, objects promoted by generation age:
    // persistent_roots[0]: Gen0 (cycle 0) → Gen1 (GC 0) → Gen2 (GC 1) → Gen2 (GC 2,3,4)
    // persistent_roots[1]: Gen0 (cycle 1) → Gen1 (GC 1) → Gen2 (GC 2) → Gen2 (GC 3,4)
    // persistent_roots[2]: Gen0 (cycle 2) → Gen1 (GC 2) → Gen2 (GC 3) → Gen2 (GC 4)
    // persistent_roots[3]: Gen0 (cycle 3) → Gen1 (GC 3) → Gen2 (GC 4)
    // persistent_roots[4]: Gen0 (cycle 4) → Gen1 (GC 4)
    assert_eq!(heap.generation_of(persistent_roots[0]), Generation::Gen2);
    assert_eq!(heap.generation_of(persistent_roots[1]), Generation::Gen2);
    assert_eq!(heap.generation_of(persistent_roots[2]), Generation::Gen2);
    assert_eq!(heap.generation_of(persistent_roots[3]), Generation::Gen2);
    // Most recent — promoted from Gen0 on last cycle
    assert_eq!(heap.generation_of(persistent_roots[4]), Generation::Gen1);
}

#[test]
fn test_forwarding_map_correctness_across_10_cycles() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    // One persistent string
    let initial = tlab.alloc_string("persistent_data".to_string());
    let mut root = initial;

    for _ in 0..10 {
        // Allocate garbage each cycle
        for j in 0..50 {
            tlab.alloc_string(format!("garbage_{j}"));
        }

        let (_, new_roots, _) =
            unsafe { heap.collect_garbage_generational(&[root], CollectionLevel::Minor) };
        tlab.invalidate();
        root = new_roots[0];

        // Verify data integrity after every cycle
        let Object::String(s) = (unsafe { root.get() }) else {
            panic!("not string")
        };
        assert_eq!(s, "persistent_data");
    }
}

#[test]
fn test_alternating_minor_major_cycles() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let a = tlab.alloc_string("a".to_string());

    // Minor: Gen0 → Gen1
    let (_, r1, _) = unsafe { heap.collect_garbage_generational(&[a], CollectionLevel::Minor) };
    tlab.invalidate();
    assert_eq!(heap.generation_of(r1[0]), Generation::Gen1);

    let b = tlab.alloc_string("b".to_string());

    // Minor: a: Gen1 → Gen2, b: Gen0 → Gen1
    let (_, r2, _) =
        unsafe { heap.collect_garbage_generational(&[r1[0], b], CollectionLevel::Minor) };
    tlab.invalidate();
    assert_eq!(heap.generation_of(r2[0]), Generation::Gen2); // a: Gen1 → Gen2
    assert_eq!(heap.generation_of(r2[1]), Generation::Gen1); // b: Gen0 → Gen1

    let c = tlab.alloc_string("c".to_string());

    // Major: flattens everything to Gen2
    let (_, r3, _) =
        unsafe { heap.collect_garbage_generational(&[r2[0], r2[1], c], CollectionLevel::Major) };
    tlab.invalidate();
    for r in &r3 {
        assert_eq!(heap.generation_of(*r), Generation::Gen2);
    }

    let d = tlab.alloc_string("d".to_string());

    // Minor: Gen2 objects untouched, new Gen0 → Gen1
    let (_, r4, _) = unsafe {
        heap.collect_garbage_generational(&[r3[0], r3[1], r3[2], d], CollectionLevel::Minor)
    };
    tlab.invalidate();
    // First three should still be Gen2
    assert_eq!(heap.generation_of(r4[0]), Generation::Gen2);
    assert_eq!(heap.generation_of(r4[1]), Generation::Gen2);
    assert_eq!(heap.generation_of(r4[2]), Generation::Gen2);
    // d should be Gen1 (promoted from Gen0)
    assert_eq!(heap.generation_of(r4[3]), Generation::Gen1);
}

// ============================================================================
// Card Table Marking Tests
// ============================================================================

#[test]
fn test_mark_card_for_gen2_object() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    // Create an object and promote to Gen2 via major GC
    let arr = tlab.alloc_array(vec![bex_vm_types::Value::Int(0)]);
    let (_, roots, _) =
        unsafe { heap.collect_garbage_generational(&[arr], CollectionLevel::Major) };
    tlab.invalidate();

    let gen2_arr = roots[0];
    assert_eq!(heap.generation_of(gen2_arr), Generation::Gen2);

    // Marking a Gen2 card should not panic — verify via subsequent GC behavior
    heap.mark_card_for_ptr(gen2_arr);

    // If card marking were broken, the next minor GC might mishandle the dirty card.
    // Run a minor GC to ensure the heap remains consistent.
    let (_, roots2, _) =
        unsafe { heap.collect_garbage_generational(&[gen2_arr], CollectionLevel::Minor) };
    tlab.invalidate();

    // Gen2 object is identity-mapped (not moved by minor GC)
    assert_eq!(roots2[0], gen2_arr);
    assert_eq!(heap.generation_of(gen2_arr), Generation::Gen2);
}

#[test]
fn test_dirty_card_roots_keep_gen0_alive_during_minor_gc() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    // Step 1: Create an array and promote to Gen2
    let arr = tlab.alloc_array(vec![bex_vm_types::Value::Null]);
    let (_, roots1, _) =
        unsafe { heap.collect_garbage_generational(&[arr], CollectionLevel::Major) };
    tlab.invalidate();
    let gen2_arr = roots1[0];
    assert_eq!(heap.generation_of(gen2_arr), Generation::Gen2);

    // Step 2: Allocate a new Gen0 string
    let young = tlab.alloc_string("young_object".to_string());
    assert_eq!(heap.generation_of(young), Generation::Gen0);

    // Step 3: Mutate the Gen2 array to reference the Gen0 string
    unsafe {
        let obj = gen2_arr.get_mut();
        if let Object::Array(arr_data) = obj {
            arr_data[0] = bex_vm_types::Value::Object(young);
        }
    }
    // Fire write barrier (Gen2 container, Gen0 ref)
    heap.mark_card_for_ptr(gen2_arr);

    // Step 4: Minor GC with NO explicit roots for `young`.
    // The ONLY way `young` can survive is via dirty card scanning.
    let (stats, new_roots, fwd) =
        unsafe { heap.collect_garbage_generational(&[gen2_arr], CollectionLevel::Minor) };
    tlab.invalidate();

    // `young` should have been discovered via dirty card scan and promoted to Gen1
    assert!(stats.promoted_to_gen1 >= 1);

    // The Gen2 array's reference should be updated via forwarding to point
    // at the promoted Gen1 copy of `young`.
    let gen2_arr_after = *fwd.get(&gen2_arr).unwrap_or(&new_roots[0]);
    let Object::Array(arr_data) = (unsafe { gen2_arr_after.get() }) else {
        panic!("Expected array")
    };
    let bex_vm_types::Value::Object(ref_ptr) = arr_data[0] else {
        panic!("Expected Object reference in slot 0")
    };
    assert_eq!(
        heap.generation_of(ref_ptr),
        Generation::Gen1,
        "young object should be promoted to Gen1 and Gen2 container's ref should be updated"
    );
    let Object::String(s) = (unsafe { ref_ptr.get() }) else {
        panic!("Expected String via the updated reference")
    };
    assert_eq!(s, "young_object");
}

#[test]
fn test_clean_card_does_not_root_gen0_objects() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    // Promote an array to Gen2 (no cross-gen references, no dirty cards)
    let arr = tlab.alloc_array(vec![bex_vm_types::Value::Int(99)]);
    let (_, roots1, _) =
        unsafe { heap.collect_garbage_generational(&[arr], CollectionLevel::Major) };
    tlab.invalidate();

    // Allocate a Gen0 object with NO reference from any Gen2 object
    let _orphan = tlab.alloc_string("orphan".to_string());

    // Minor GC: orphan is not rooted, no dirty cards point to it
    let (stats, _, _) =
        unsafe { heap.collect_garbage_generational(&[roots1[0]], CollectionLevel::Minor) };
    tlab.invalidate();

    // orphan should be collected (plus TLAB pre-alloc padding)
    assert!(stats.collected_count >= 1);
}

/// A dirty card on a Gen2 object that holds no young references must not
/// spuriously keep a later Gen0 allocation alive on the next Minor GC.
///
/// Minor GC does **not** proactively clear the Gen2 card table — only Major
/// GC does ([`gc.rs:206`]) — so a synthetically-marked card survives across
/// Minor GCs. That is still correct as long as the card scan only promotes
/// things that are actually referenced from the card's objects; this test
/// pins that behaviour.
#[test]
fn test_stale_dirty_card_does_not_root_unrelated_gen0_object() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    // Promote an array with no object references into Gen2.
    let arr = tlab.alloc_array(vec![bex_vm_types::Value::Null]);
    let (_, roots1, _) =
        unsafe { heap.collect_garbage_generational(&[arr], CollectionLevel::Major) };
    tlab.invalidate();

    // Mark a dirty card on the promoted Gen2 object and sanity-check that the
    // introspection helper reports it.
    heap.mark_card_for_ptr(roots1[0]);
    assert_eq!(
        heap.gen2_dirty_card_count(),
        1,
        "marking a Gen2 card should register exactly one dirty card",
    );

    // First minor GC — the dirty card is scanned but the Gen2 object has no
    // young references, so nothing is promoted through it.
    let (_, roots2, _) =
        unsafe { heap.collect_garbage_generational(&[roots1[0]], CollectionLevel::Minor) };
    tlab.invalidate();

    // The synthetic dirty card persists (Minor GC doesn't clear the table),
    // which is still correct: next Minor GC will re-scan it and find nothing.
    assert_eq!(
        heap.gen2_dirty_card_count(),
        1,
        "Minor GC does not clear the Gen2 card table — the mark persists",
    );

    // Allocate a brand-new Gen0 object that nothing references.
    let _orphan2 = tlab.alloc_string("orphan2".to_string());

    // Second minor GC: the old dirty card's Gen2 object does NOT reference
    // orphan2, so orphan2 must be collected despite the dirty card being
    // scanned again.
    let (stats, _, _) =
        unsafe { heap.collect_garbage_generational(&[roots2[0]], CollectionLevel::Minor) };
    tlab.invalidate();

    assert!(
        stats.collected_count >= 1,
        "orphan2 must be collected — a dirty card on an unrelated Gen2 object must not root it"
    );
}

// ============================================================================
// Gen2 Stale Pointer Tests (Phase 5.1)
// ============================================================================

/// A pre-existing Gen2 container holds a reference to a Gen0 object.
/// After a minor GC the Gen0 object is promoted to Gen1 and the Gen2
/// container's reference should be updated to point at the new Gen1 copy.
#[test]
fn test_critical_gen2_stale_pointer_after_minor_gc() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    // Step 1: Allocate a Gen0 array and promote it to Gen2 via major GC.
    let container = tlab.alloc_array(vec![bex_vm_types::Value::Null]);
    let (_, roots1, _) =
        unsafe { heap.collect_garbage_generational(&[container], CollectionLevel::Major) };
    tlab.invalidate();
    let gen2_container = roots1[0];
    assert_eq!(heap.generation_of(gen2_container), Generation::Gen2);

    // Step 2: Allocate a young Gen0 string.
    let young = tlab.alloc_string("young_value".to_string());
    assert_eq!(heap.generation_of(young), Generation::Gen0);

    // Step 3: Mutate the Gen2 array to reference the Gen0 string,
    //         then fire the write barrier (dirty card).
    unsafe {
        let obj = gen2_container.get_mut();
        if let Object::Array(arr_data) = obj {
            arr_data[0] = bex_vm_types::Value::Object(young);
        }
    }
    heap.mark_card_for_ptr(gen2_container);

    // Step 4: Minor GC without rooting `young` explicitly.
    //         `young` must survive via dirty-card scan and be promoted to Gen1.
    let (stats, new_roots, fwd) =
        unsafe { heap.collect_garbage_generational(&[gen2_container], CollectionLevel::Minor) };
    tlab.invalidate();

    // The young object should have been discovered and promoted.
    assert!(stats.promoted_to_gen1 >= 1);

    // The Gen2 container's reference should now point to the Gen1 copy of young.
    let gen2_after = *fwd.get(&gen2_container).unwrap_or(&new_roots[0]);
    let Object::Array(arr_data) = (unsafe { gen2_after.get() }) else {
        panic!("Expected Array in Gen2 container")
    };
    let bex_vm_types::Value::Object(ref_ptr) = arr_data[0] else {
        panic!("Expected Object reference in slot 0")
    };
    assert_eq!(
        heap.generation_of(ref_ptr),
        Generation::Gen1,
        "young object should be promoted to Gen1 and Gen2 container's ref updated"
    );
    let Object::String(s) = (unsafe { ref_ptr.get() }) else {
        panic!("Expected String via the updated reference")
    };
    assert_eq!(s, "young_value");
}

/// A Gen2 container holds a reference to a Gen0 object which itself
/// references another Gen0 object (a chain through Gen0 from Gen2).
/// After minor GC, both Gen0 objects should be promoted to Gen1 and the
/// Gen2 container's reference chain should be fully updated.
///
/// Same root cause as `test_critical_gen2_stale_pointer_after_minor_gc`.
#[test]
fn test_critical_gen2_chain_through_gen0_after_minor_gc() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    // Step 1: Promote an array container to Gen2.
    let container = tlab.alloc_array(vec![bex_vm_types::Value::Null]);
    let (_, roots1, _) =
        unsafe { heap.collect_garbage_generational(&[container], CollectionLevel::Major) };
    tlab.invalidate();
    let gen2_container = roots1[0];
    assert_eq!(heap.generation_of(gen2_container), Generation::Gen2);

    // Step 2: Build a Gen0 chain: inner_leaf <- wrapper_array
    let inner_leaf = tlab.alloc_string("inner_leaf".to_string());
    let wrapper = tlab.alloc_array(vec![bex_vm_types::Value::Object(inner_leaf)]);
    assert_eq!(heap.generation_of(inner_leaf), Generation::Gen0);
    assert_eq!(heap.generation_of(wrapper), Generation::Gen0);

    // Step 3: Make the Gen2 container point at the Gen0 wrapper, fire write barrier.
    unsafe {
        let obj = gen2_container.get_mut();
        if let Object::Array(arr_data) = obj {
            arr_data[0] = bex_vm_types::Value::Object(wrapper);
        }
    }
    heap.mark_card_for_ptr(gen2_container);

    // Step 4: Minor GC without explicit roots for the Gen0 objects.
    let (stats, new_roots, fwd) =
        unsafe { heap.collect_garbage_generational(&[gen2_container], CollectionLevel::Minor) };
    tlab.invalidate();

    // Both Gen0 objects should have been discovered via dirty-card scan.
    assert!(stats.promoted_to_gen1 >= 2);

    // Navigate the Gen2 container's updated reference chain.
    let gen2_after = *fwd.get(&gen2_container).unwrap_or(&new_roots[0]);
    let Object::Array(arr_data) = (unsafe { gen2_after.get() }) else {
        panic!("Expected Array in Gen2 container")
    };
    let bex_vm_types::Value::Object(wrapper_ptr) = arr_data[0] else {
        panic!("Expected Object in slot 0 of Gen2 container")
    };
    assert_eq!(
        heap.generation_of(wrapper_ptr),
        Generation::Gen1,
        "wrapper should be promoted to Gen1"
    );

    // Navigate inside the wrapper to find the inner leaf.
    let Object::Array(wrapper_arr) = (unsafe { wrapper_ptr.get() }) else {
        panic!("Expected Array for wrapper")
    };
    let bex_vm_types::Value::Object(leaf_ptr) = wrapper_arr[0] else {
        panic!("Expected Object in slot 0 of wrapper")
    };
    assert_eq!(
        heap.generation_of(leaf_ptr),
        Generation::Gen1,
        "inner leaf should be promoted to Gen1"
    );
    let Object::String(s) = (unsafe { leaf_ptr.get() }) else {
        panic!("Expected String for inner leaf")
    };
    assert_eq!(s, "inner_leaf");
}

// ============================================================================
// Handle Root Coverage Tests (Phase 5.2)
// ============================================================================

/// Handles are treated as GC roots: objects referenced only through a handle
/// (not through any stack root) must survive GC.
#[test]
fn test_handle_roots_included_in_gc() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let obj = tlab.alloc_string("handle_only".to_string());

    // Create a handle — this is the only reference to `obj`.
    let handle = heap.create_handle(obj);

    // Include handle roots in GC roots (no explicit stack roots for `obj`).
    let mut roots = heap.collect_handle_roots();
    let (stats, _, forwarding) =
        unsafe { heap.collect_garbage_generational(&roots, CollectionLevel::Minor) };
    tlab.invalidate();

    // `obj` should have survived (promoted to Gen1 from Gen0).
    assert!(stats.live_count >= 1);

    // Update roots from forwarding (simulate what the engine does).
    for r in &mut roots {
        if let Some(&new_ptr) = forwarding.get(r) {
            *r = new_ptr;
        }
    }

    // The handle should resolve to a valid Gen1 pointer.
    let resolved = heap.resolve_handle_ptr(handle.slab_key());
    assert!(resolved.is_some(), "handle should still resolve after GC");
    let resolved_ptr = resolved.unwrap();
    assert_eq!(heap.generation_of(resolved_ptr), Generation::Gen1);

    let Object::String(s) = (unsafe { resolved_ptr.get() }) else {
        panic!("Expected String through handle")
    };
    assert_eq!(s, "handle_only");
}

/// Handles are GC roots by contract: passing an empty root set while a
/// handle exists is a caller bug and must panic rather than silently leave
/// the handle pointing at reclaimed memory.
#[test]
#[should_panic(expected = "handles must be passed as GC roots")]
fn test_minor_gc_panics_when_handle_not_in_roots() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let dead = tlab.alloc_string("dead_object".to_string());
    let _handle = heap.create_handle(dead);

    // Caller forgot to include `collect_handle_roots()` — must panic.
    let _ = unsafe { heap.collect_garbage_generational(&[], CollectionLevel::Minor) };
}

/// An object promoted to Gen1 via handle roots during a minor GC is
/// accessible through the handle after GC and has the correct generation.
#[test]
fn test_handle_survives_minor_gc_and_points_to_gen1() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let obj = tlab.alloc_string("gen1_via_handle".to_string());
    assert_eq!(heap.generation_of(obj), Generation::Gen0);

    let handle = heap.create_handle(obj);

    // Use handle roots to drive the minor GC.
    let handle_roots = heap.collect_handle_roots();
    assert_eq!(handle_roots.len(), 1);

    let (stats, _, _) =
        unsafe { heap.collect_garbage_generational(&handle_roots, CollectionLevel::Minor) };
    tlab.invalidate();

    assert_eq!(stats.promoted_to_gen1, 1);

    let resolved = heap.resolve_handle_ptr(handle.slab_key()).unwrap();
    assert_eq!(heap.generation_of(resolved), Generation::Gen1);

    let Object::String(s) = (unsafe { resolved.get() }) else {
        panic!("Expected String through handle after Gen1 promotion")
    };
    assert_eq!(s, "gen1_via_handle");
}

/// A Gen1→Gen2 promotion must not leave a Gen2 object with untracked young
/// references. If the container's outgoing references include pointers into
/// the freshly-promoted-from-Gen0 new-Gen1, the container's card in the Gen2
/// card table must be dirty after the GC completes — otherwise the next
/// Minor GC will identity-map this Gen2 container without tracing its
/// references, and the referenced young objects will be swept with old Gen1.
///
/// This scenario arises naturally whenever a Gen1 container acquires a
/// young reference via mutation: the write barrier fires but `mark_card_for_ptr`
/// only records cards for Gen2 containers, so no card survives to the next
/// GC. The required recovery happens at promotion time, not at mutation
/// time.
///
/// The test walks three Minor GCs to exercise the promotion-and-survival
/// chain end-to-end and asserts the Gen1 child survives with its original
/// value.
#[test]
fn test_gen1_container_acquires_young_ref_survives_minor_gc_chain() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    // T0: Allocate container A in Gen0 with a placeholder slot.
    let a_g0 = tlab.alloc_array(vec![bex_vm_types::Value::Null]);
    assert_eq!(heap.generation_of(a_g0), Generation::Gen0);

    // GC1 Minor: A → Gen1.
    let (_, roots1, _) =
        unsafe { heap.collect_garbage_generational(&[a_g0], CollectionLevel::Minor) };
    tlab.invalidate();
    let a_g1 = roots1[0];
    assert_eq!(heap.generation_of(a_g1), Generation::Gen1);

    // T2: Allocate B in Gen0, then mutate A (Gen1) to reference B.
    // The standard write barrier fires but is a no-op: mark_card_for_ptr is
    // only implemented for Gen2 containers. So no card is dirty.
    let b_g0 = tlab.alloc_string("B_contents".to_string());
    assert_eq!(heap.generation_of(b_g0), Generation::Gen0);
    unsafe {
        let obj = a_g1.get_mut();
        let Object::Array(arr) = obj else {
            panic!("expected Array")
        };
        arr[0] = bex_vm_types::Value::Object(b_g0);
    }

    // GC2 Minor: only A is a root. B reached via A's references.
    // A promotes to Gen2, B promotes to new Gen1.
    // Post-fix: the newly-promoted Gen2 A's card is marked dirty.
    let (stats_gc2, roots2, _) =
        unsafe { heap.collect_garbage_generational(&[a_g1], CollectionLevel::Minor) };
    tlab.invalidate();
    let a_g2 = roots2[0];
    assert_eq!(heap.generation_of(a_g2), Generation::Gen2);
    assert_eq!(stats_gc2.promoted_to_gen2, 1);
    assert_eq!(stats_gc2.promoted_to_gen1, 1);

    // GC3 Minor: only A is a root. Without the post-promotion card mark, A
    // would be identity-mapped and B would be lost (dropped with old Gen1).
    // With the fix, A's dirty card is scanned, B is discovered, and B is
    // promoted again (to new Gen1).
    let (_, roots3, _) =
        unsafe { heap.collect_garbage_generational(&[a_g2], CollectionLevel::Minor) };
    tlab.invalidate();
    let a_after = roots3[0];

    // A must still resolve to a valid Array, and its slot 0 must still
    // resolve to a valid String matching B's original contents.
    let Object::Array(arr_after) = (unsafe { a_after.get() }) else {
        panic!("expected Array in A after GC3")
    };
    let bex_vm_types::Value::Object(b_after) = arr_after[0] else {
        panic!("expected Object reference in A.field after GC3")
    };
    let Object::String(s) = (unsafe { b_after.get() }) else {
        panic!("expected String at B_after")
    };
    assert_eq!(
        s, "B_contents",
        "B must survive two minor GCs via the post-promotion card mark"
    );
}
