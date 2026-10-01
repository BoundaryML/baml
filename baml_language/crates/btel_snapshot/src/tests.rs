use std::sync::Arc;

use num_bigint::BigInt;

use super::*;

/// A byte array's object, and the handle that reads its bytes back.
fn bytes(b: &mut Builder, content: &[u8]) -> (SnapshotObject, Uint8ArrayData) {
    let object = b.bytes(content);
    let SnapshotObject::Uint8Array { data } = &object else {
        panic!("the bytes fit the arena")
    };
    let data = *data;
    (object, data)
}

fn scalar(pool: &SnapshotPool, n: i64) -> Snapshot {
    pool.try_acquire()
        .unwrap()
        .finish(SnapshotValue::Int(n), &mut Shaper::default())
}
#[test]
fn capacity_counts_owners_across_threads_and_reuse_has_no_stale_values() {
    let pool = SnapshotPool::new(1, Limits::default());
    for n in 0..100 {
        let snapshot = scalar(&pool, n);
        assert!(pool.try_acquire().is_none());
        assert_eq!(pool.stats().in_use, 1);
        std::thread::spawn(move || {
            assert_eq!(snapshot.roots().len(), 1);
            assert!(matches!(snapshot.roots()[0],SnapshotValue::Int(x) if x==n));
        })
        .join()
        .unwrap();
        assert_eq!(pool.stats().in_use, 0);
    }
    assert_eq!(pool.stats().allocation_misses, 1);
}
#[test]
fn immutable_leaves_and_pool_survive_the_original_owners() {
    let leaf = Arc::new(BigInt::from(123));
    let weak = Arc::downgrade(&leaf);
    let pool = SnapshotPool::new(1, Limits::default());
    let mut b = pool.try_acquire().unwrap();
    let SnapshotValue::Bigint(id) = b.leaves().bigint(&leaf) else {
        panic!("the bigint is held")
    };
    let snapshot = b.finish(SnapshotValue::Bigint(id), &mut Shaper::default());
    drop(leaf);
    drop(pool);
    assert!(weak.upgrade().is_some());
    assert_eq!(**snapshot.bigint(id), BigInt::from(123));
    drop(snapshot);
    assert!(weak.upgrade().is_none());
}
#[test]
fn unwind_releases_builder_and_large_allocations_are_not_retained() {
    let pool = SnapshotPool::new(1, Limits::default());
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _b = pool.try_acquire().unwrap();
            panic!("build failed");
        }))
        .is_err()
    );
    let mut b = pool.try_acquire().unwrap();
    let data = vec![9; 512 * 1024];
    let (object, range) = bytes(&mut b, &data);
    let id = b.leaves().object(object).unwrap();
    let snapshot = b.finish(SnapshotValue::Object(id), &mut Shaper::default());
    assert_eq!(snapshot.bytes(range), data);
    assert!(pool.stats().allocated_bytes > data.len());
    drop(snapshot);
    assert_eq!(pool.stats().allocated, 0);
    assert_eq!(pool.stats().allocated_bytes, 0);
    let snapshot = scalar(&pool, 9);
    assert_eq!(snapshot.roots().len(), 1);
}
#[test]
fn limits_are_explicit_and_what_they_cut_shows_in_the_capture() {
    let pool = SnapshotPool::new(
        1,
        Limits {
            max_objects: Some(1),
            max_values: Some(2),
            max_depth: Some(2),
        },
    );
    let mut b = pool.try_acquire().unwrap();
    let (object, range) = bytes(&mut b, b"abcde");
    let id = b.leaves().object(object).unwrap();
    assert!(b.leaves().reserve().is_none());
    // A list takes the items the value limit allows and records how many
    // its source had.
    let ty = b.leaves().ty(baml_type::RealizedTy::Int);
    let list = b.list(ty, [1, 2, 3].into_iter(), |_, n| SnapshotValue::Int(n));
    assert!(list.is_cut());
    assert!(matches!(
        list,
        SnapshotObject::List { items, original_len: 3, .. } if items.len() == 2
    ));
    let whole = b.finish(SnapshotValue::Object(id), &mut Shaper::default());
    assert_eq!(whole.bytes(range), b"abcde");
    let stats = whole.stats();
    assert!(!stats.limited);
    assert_eq!((stats.copied_bytes, stats.shared_bytes), (5, 0));
    drop(whole);

    // A storage that is used again keeps nothing of the capture before.
    let mut b = pool.try_acquire().unwrap();
    let held = "h".repeat(100);
    let text = b.leaves().string_value(&held.as_str().into());
    // A cut value anywhere in the capture marks it.
    b.leaves()
        .object(SnapshotObject::Cell(SnapshotValue::Truncated(Limit::Depth)))
        .unwrap();
    let cut = b.finish(text, &mut Shaper::default());
    let stats = cut.stats();
    assert!(stats.limited);
    assert_eq!((stats.copied_bytes, stats.shared_bytes), (0, 100));
}

#[test]
fn growth_accounts_for_replacement_overlap_and_ignores_idle_budget() {
    let pool = SnapshotPool::with_config(
        1,
        Limits::default(),
        PoolConfig {
            retention_budget_bytes: 1,
            ..PoolConfig::default()
        },
    );
    let mut b = pool.try_acquire().unwrap();
    let (_, first) = bytes(&mut b, &[7; 512]);
    let (object, second) = bytes(&mut b, &[9; 513]);
    // Check immediately: later arena allocations can raise the live total.
    let growth_stats = pool.stats();
    assert!(growth_stats.peak_accounted_bytes >= growth_stats.allocated_bytes + 512);
    let id = b.leaves().object(object).unwrap();
    let snapshot = b.finish(SnapshotValue::Object(id), &mut Shaper::default());
    assert_eq!(snapshot.bytes(first), &[7; 512]);
    assert_eq!(snapshot.bytes(second), &[9; 513]);
    let stats = pool.stats();
    assert_eq!(stats.idle_bytes, 0);
    drop(snapshot);
    assert_eq!(pool.stats().allocated_bytes, 0);
    assert_eq!(pool.stats().allocated, 0);
}

#[test]
fn idle_budget_is_total_not_per_buffer_and_relocation_preserves_owners() {
    let budget = 4096;
    let pool = SnapshotPool::with_config(
        3,
        Limits::default(),
        PoolConfig {
            retention_budget_bytes: budget,
            max_retained_allocation_bytes: usize::MAX,
            ..PoolConfig::default()
        },
    );
    let leaf = Arc::new(BigInt::from(99));
    let weak = Arc::downgrade(&leaf);
    let mut b = pool.try_acquire().unwrap();
    let SnapshotValue::Bigint(root) = b.leaves().bigint(&leaf) else {
        panic!("the bigint is held")
    };
    // Move initialized Arc owners during arena growth; indices stay valid.
    for _ in 0..256 {
        b.leaves().bigint(&leaf);
    }
    let a = b.finish(SnapshotValue::Bigint(root), &mut Shaper::default());
    let b = scalar(&pool, 2);
    let c = scalar(&pool, 3);
    assert_eq!(**a.bigint(root), BigInt::from(99));
    drop(leaf);
    drop(a);
    assert!(weak.upgrade().is_none());
    drop(b);
    drop(c);
    let stats = pool.stats();
    assert!(stats.idle_bytes <= budget);
    assert_eq!(stats.in_use, 0);
    assert_eq!(stats.allocated_bytes, stats.idle_bytes);
    assert!(
        stats.allocated < 3,
        "oversized allocation must not permanently inflate pool"
    );
}

#[test]
fn content_ids_include_value_kinds_omissions_and_leaf_contents() {
    let pool = SnapshotPool::new(1, Limits::default());
    let id = |v| {
        pool.try_acquire()
            .unwrap()
            .finish(v, &mut Shaper::default())
            .root_id()
    };
    let values = [
        SnapshotValue::Null,
        SnapshotValue::OmittedArg,
        SnapshotValue::Bool(false),
        SnapshotValue::Int(0),
        SnapshotValue::Float(0.0),
        SnapshotValue::Float(-0.0),
        SnapshotValue::Truncated(Limit::Bytes),
        SnapshotValue::Truncated(Limit::Depth),
    ];
    let ids: std::collections::HashSet<_> = values.into_iter().map(id).collect();
    assert_eq!(ids.len(), values.len());
    let string_id = |s: &BexStr| {
        let mut b = pool.try_acquire().unwrap();
        let value = b.leaves().string(s).unwrap();
        b.finish(SnapshotValue::String(value), &mut Shaper::default())
            .root_id()
    };
    let rope = BexStr::concat("a".repeat(60).into(), "b".repeat(60).into());
    let flat = BexStr::from(format!("{}{}", "a".repeat(60), "b".repeat(60)));
    assert_eq!(string_id(&rope), string_id(&flat));
    assert_ne!(string_id(&flat), string_id(&"different".into()));
    let big_id = |n| {
        let mut b = pool.try_acquire().unwrap();
        let value = b.leaves().bigint(&Arc::new(BigInt::from(n)));
        b.finish(value, &mut Shaper::default()).root_id()
    };
    assert_ne!(big_id(1), big_id(-1));
    assert_ne!(big_id(1), id(SnapshotValue::Int(1)));
}

#[test]
fn graph_hashes_preserve_cycles_aliases_and_reset_on_reuse() {
    let pool = SnapshotPool::new(1, Limits::default());
    let make = |distinct: bool| {
        let mut b = pool.try_acquire().unwrap();
        let slot = b.leaves().reserve().unwrap();
        let root = SnapshotValue::Object(slot.id());
        let cell = |b: &mut Builder| b.leaves().object(SnapshotObject::Cell(root)).unwrap();
        let first = cell(&mut b);
        let second = if distinct { cell(&mut b) } else { first };
        let ty = b.leaves().ty(baml_type::RealizedTy::Int);
        let list = b.list(ty, [first, second].into_iter(), |_, id| {
            SnapshotValue::Object(id)
        });
        b.fill(slot, list);
        b.finish(root, &mut Shaper::default()).root_id()
    };
    let aliased = make(false);
    let separate = make(true);
    assert_ne!(aliased, separate);
    assert_eq!(make(false), aliased);
    assert_eq!(make(true), separate);
    assert_eq!(pool.stats().allocation_misses, 1);
}

fn root_blob_bytes(snapshot: &Snapshot) -> Vec<u8> {
    let mut bytes = Vec::new();
    snapshot
        .root_blob()
        .write(&mut BlobScratch::default(), &mut bytes)
        .unwrap();
    bytes
}

#[test]
fn blobs_ignore_discovery_order_unreachable_objects_and_reused_scratch() {
    let pool = SnapshotPool::new(1, Limits::default());
    let mut reused = Shaper::default();
    // A list of two cells, the second pointing at the first, with its objects
    // reserved in either order and optionally beside an unreachable object.
    let capture = |reverse: bool, stray: bool, shaper: &mut Shaper| {
        let mut b = pool.try_acquire().unwrap();
        let ty = b.leaves().ty(baml_type::RealizedTy::Int);
        if stray {
            b.leaves()
                .object(SnapshotObject::Cell(SnapshotValue::Int(9)))
                .unwrap();
        }
        let mut slots = [(); 3].map(|()| b.leaves().reserve().unwrap());
        if reverse {
            slots.reverse();
        }
        let [list, first, second] = slots;
        let (root, first_id, second_id) = (list.id(), first.id(), second.id());
        b.fill(first, SnapshotObject::Cell(SnapshotValue::Int(1)));
        b.fill(
            second,
            SnapshotObject::Cell(SnapshotValue::Object(first_id)),
        );
        let items = b.list(ty, [first_id, second_id].into_iter(), |_, id| {
            SnapshotValue::Object(id)
        });
        b.fill(list, items);
        let snapshot = b.finish(SnapshotValue::Object(root), shaper);
        assert_eq!(snapshot.blobs().len(), 1);
        (snapshot.root_id(), root_blob_bytes(&snapshot))
    };
    let canonical = capture(false, false, &mut Shaper::default());
    for (reverse, stray) in [(true, false), (false, true), (true, true), (false, false)] {
        assert_eq!(capture(reverse, stray, &mut reused), canonical);
    }
    assert_eq!(pool.stats().in_use, 0);
}
