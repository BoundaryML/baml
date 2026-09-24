use super::*;

fn scalar(pool: &SnapshotPool, n: i64) -> Snapshot {
    pool.try_acquire()
        .unwrap()
        .finish_value(SnapshotValue::Int(n))
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
    let id = b.bigint(&leaf).unwrap();
    let snapshot = b.finish_value(SnapshotValue::Bigint(id));
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
    let range = b.copy_bytes(&data);
    let id = b.reserve_object().unwrap();
    b.set_object(
        id,
        SnapshotObject::Bytes {
            data: range,
            original_len: data.len(),
        },
    );
    let snapshot = b.finish_value(SnapshotValue::Object(id));
    assert_eq!(snapshot.bytes(range), data);
    assert!(pool.stats().allocated_bytes > data.len());
    drop(snapshot);
    assert_eq!(pool.stats().allocated, 0);
    assert_eq!(pool.stats().allocated_bytes, 0);
    let snapshot = scalar(&pool, 9);
    assert_eq!(snapshot.roots().len(), 1);
}
#[test]
fn limits_are_explicit_and_recycled_handles_are_released() {
    let pool = SnapshotPool::new(
        1,
        Limits {
            max_objects: Some(1),
            max_values: None,
            max_bytes: Some(3),
            max_depth: Some(2),
        },
    );
    let mut b = pool.try_acquire().unwrap();
    let range = b.copy_bytes(b"abcde");
    let id = b.reserve_object().unwrap();
    b.set_object(
        id,
        SnapshotObject::Bytes {
            data: range,
            original_len: 5,
        },
    );
    assert!(b.reserve_object().is_none());
    let snapshot = b.finish_value(SnapshotValue::Object(id));
    assert!(snapshot.stats().limited);
    assert_eq!(snapshot.bytes(range), b"abc");
    drop(snapshot);
    let snapshot = scalar(&pool, 1);
    assert!(!snapshot.stats().limited);
    assert_eq!(snapshot.stats().copied_bytes, 0);
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
    let first = b.copy_bytes(&[7; 512]);
    let second = b.copy_bytes(&[9; 513]);
    // Check immediately: later arena allocations can raise the live total.
    let growth_stats = pool.stats();
    assert!(growth_stats.peak_accounted_bytes >= growth_stats.allocated_bytes + 512);
    let id = b.reserve_object().unwrap();
    b.set_object(
        id,
        SnapshotObject::Bytes {
            data: second,
            original_len: 513,
        },
    );
    let snapshot = b.finish_value(SnapshotValue::Object(id));
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
    let root = b.bigint(&leaf).unwrap();
    // Move initialized Arc owners during arena growth; indices stay valid.
    for _ in 0..256 {
        b.bigint(&leaf).unwrap();
    }
    let a = b.finish_value(SnapshotValue::Bigint(root));
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
    let id = |v| pool.try_acquire().unwrap().finish_value(v).id();
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
        let value = b.string(s).unwrap();
        b.finish_value(SnapshotValue::String(value)).id()
    };
    let rope = BexStr::concat("a".repeat(60).into(), "b".repeat(60).into());
    let flat = BexStr::from(format!("{}{}", "a".repeat(60), "b".repeat(60)));
    assert_eq!(string_id(&rope), string_id(&flat));
    assert_ne!(string_id(&flat), string_id(&"different".into()));
    let big_id = |n| {
        let mut b = pool.try_acquire().unwrap();
        let value = b.bigint(&Arc::new(BigInt::from(n))).unwrap();
        b.finish_value(SnapshotValue::Bigint(value)).id()
    };
    assert_ne!(big_id(1), big_id(-1));
    assert_ne!(big_id(1), id(SnapshotValue::Int(1)));
}

#[test]
fn graph_hashes_preserve_cycles_aliases_and_reset_on_reuse() {
    let pool = SnapshotPool::new(1, Limits::default());
    let make = |distinct: bool| {
        let mut b = pool.try_acquire().unwrap();
        let root = b.reserve_object().unwrap();
        let first = b.reserve_object().unwrap();
        let second = if distinct {
            b.reserve_object().unwrap()
        } else {
            first
        };
        b.set_object(first, SnapshotObject::Cell(SnapshotValue::Object(root)));
        b.set_object(second, SnapshotObject::Cell(SnapshotValue::Object(root)));
        let ty = b.push_type(baml_type::RealizedTy::Int);
        let start = b.value_start();
        b.push_value(SnapshotValue::Object(first));
        b.push_value(SnapshotValue::Object(second));
        let items = b.value_range(start);
        b.set_object(
            root,
            SnapshotObject::List {
                element_type: ty,
                items,
                original_len: 2,
            },
        );
        b.finish_value(SnapshotValue::Object(root)).id()
    };
    let aliased = make(false);
    let separate = make(true);
    assert_ne!(aliased, separate);
    assert_eq!(make(false), aliased);
    assert_eq!(make(true), separate);
    assert_eq!(pool.stats().allocation_misses, 1);
}
