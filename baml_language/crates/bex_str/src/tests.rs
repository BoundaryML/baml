use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

use super::bex_str::BexStr;

fn hash_of<T: Hash>(t: &T) -> u64 {
    let mut h = DefaultHasher::new();
    t.hash(&mut h);
    h.finish()
}

#[test]
fn size_assertion() {
    assert_eq!(std::mem::size_of::<BexStr>(), 56);
}

#[test]
fn inline_boundary() {
    // 54 bytes → Inline
    let s54 = "a".repeat(54);
    let b54 = BexStr::from(s54.as_str());
    assert!(matches!(b54, BexStr::Inline { .. }));

    // 55 bytes → Flat
    let s55 = "a".repeat(55);
    let b55 = BexStr::from(s55.as_str());
    assert!(matches!(b55, BexStr::Flat(_)));
}

#[test]
fn clone_semantics() {
    // Flat clone shares Arc
    let s = BexStr::from("a".repeat(100));
    if let BexStr::Flat(ref arc1) = s {
        let cloned = s.clone();
        if let BexStr::Flat(ref arc2) = cloned {
            assert!(Arc::ptr_eq(arc1, arc2));
        }
    }
}

#[test]
fn concat_deep_tree_and_flatten() {
    let mut s = BexStr::from("x");
    for _ in 0..1000 {
        s = BexStr::concat(s, BexStr::from("y"));
    }
    let expected = format!("x{}", "y".repeat(1000));
    assert_eq!(s.as_str(), expected);
}

#[test]
fn concat_deep_drop_no_stack_overflow() {
    let mut s = BexStr::from("a");
    for _ in 0..50_000 {
        s = BexStr::concat(s, BexStr::from("b"));
    }
    drop(s); // Should not stack overflow
}

#[test]
fn slice_depth_one_invariant() {
    let long = BexStr::from("a".repeat(200));
    let s1 = long.substring(10, 100);
    assert!(matches!(s1, BexStr::Slice { .. }));

    // Re-slice: still depth-1, same parent
    let s2 = s1.substring(0, 80);
    if let (BexStr::Slice { parent: p1, .. }, BexStr::Slice { parent: p2, .. }) = (&s1, &s2) {
        assert!(Arc::ptr_eq(p1, p2));
    }
}

#[test]
fn hash_consistency_with_str() {
    let text = "hello world";
    let bex = BexStr::from(text);
    assert_eq!(hash_of(&bex), hash_of(&text.to_string()));
}

#[test]
#[allow(clippy::mutable_key_type)]
fn borrow_str_map_lookup() {
    let mut map = HashMap::new();
    map.insert(BexStr::from("key"), 42);
    assert_eq!(map.get("key"), Some(&42));
}

#[test]
fn partial_ord_matches_string() {
    let a = BexStr::from("apple");
    let b = BexStr::from("banana");
    assert!(a < b);
    assert_eq!(a.cmp(&b), "apple".cmp("banana"));
}

#[test]
fn empty_string() {
    let e = BexStr::empty();
    assert_eq!(e.len(), 0);
    assert!(e.is_empty());
    assert_eq!(e.as_str(), "");
}

#[test]
fn from_empty_string() {
    let e = BexStr::from("");
    assert!(matches!(e, BexStr::Inline { len: 0, .. }));
    assert_eq!(e.as_str(), "");
}
