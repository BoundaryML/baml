use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

use indexmap::IndexMap;

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

#[test]
#[allow(clippy::mutable_key_type)]
fn borrow_str_indexmap_lookup() {
    let mut map: IndexMap<BexStr, i32> = IndexMap::new();
    map.insert(BexStr::from("key"), 42);
    // Lookup via &str — exercises Borrow<str> contract on IndexMap
    assert_eq!(map.get("key"), Some(&42));
}

#[test]
fn hash_consistency_bexstr_vs_str() {
    // BexStr and &str must produce identical hashes via std Hasher
    // — required by the Borrow<str> contract.
    let text = "hello world";
    assert_eq!(hash_of(&BexStr::from(text)), hash_of(&text));
}

#[test]
fn concat_flatten_idempotent() {
    let a = BexStr::from("hello ");
    let b = BexStr::from("world");
    let c = BexStr::concat(a, b);
    // First access flattens the Concat node.
    let s1 = c.as_str().to_owned();
    // Second access must return the same content (flatten is cached).
    let s2 = c.as_str().to_owned();
    assert_eq!(s1, s2);
    assert_eq!(s1, "hello world");
}

#[test]
fn flattening_parent_concat_preserves_shared_child_concat() {
    let name = "a".repeat(41);
    let full_name = BexStr::concat(
        BexStr::from("levenshtein distance/"),
        BexStr::from(name.as_str()),
    );
    let expected = format!("levenshtein distance/{name}");
    assert_eq!(full_name.len(), expected.len());
    assert_eq!(full_name.as_str(), expected);

    // This mirrors the test registry's `hash_prefix = full_name + "#"` path:
    // flattening the derived parent must not corrupt the stored full name.
    let hash_prefix = BexStr::concat(full_name.clone(), BexStr::from("#"));
    assert_eq!(hash_prefix.as_str(), format!("{expected}#"));
    assert_eq!(full_name.as_str(), expected);
}

#[test]
fn empty_concat_variants() {
    let e = BexStr::empty();
    let a = BexStr::from("hello");
    // empty + non-empty → identity (returns the non-empty side directly)
    assert_eq!(BexStr::concat(e.clone(), a.clone()).as_str(), "hello");
    // non-empty + empty → identity
    assert_eq!(BexStr::concat(a.clone(), e.clone()).as_str(), "hello");
    // empty + empty → empty
    assert_eq!(BexStr::concat(e.clone(), e.clone()).as_str(), "");
}

#[test]
fn mixed_variant_concat() {
    let inline = BexStr::from("hi"); // Inline (2 bytes)
    let flat = BexStr::from("a".repeat(100)); // Flat
    let slice = flat.substring(10, 50); // Slice (40 bytes)
    let c = BexStr::concat(inline, slice);
    assert_eq!(c.len(), 2 + 40);
    assert_eq!(&c.as_str()[..2], "hi");
    assert_eq!(&c.as_str()[2..], &"a".repeat(40));
}

#[test]
fn char_count_ascii() {
    let s = BexStr::from("hello");
    assert_eq!(s.char_count(), 5);
    assert_eq!(s.len(), 5);
}

#[test]
fn char_count_multibyte() {
    let s = BexStr::from("héllo");
    assert_eq!(s.char_count(), 5);
    assert_eq!(s.len(), 6); // é is 2 bytes
}

#[test]
fn char_count_emoji() {
    let s = BexStr::from("😀hello");
    assert_eq!(s.char_count(), 6);
    assert_eq!(s.len(), 9); // 😀 is 4 bytes + "hello" is 5
}

#[test]
fn char_count_flat() {
    let s = BexStr::from("a".repeat(100) + "😀");
    assert_eq!(s.char_count(), 101);
    assert_eq!(s.len(), 104);
}

#[test]
fn char_count_slice() {
    // substring takes BYTE offsets at the BexStr level
    let s = BexStr::from("hello 😀 world");
    // "hello " = 6 bytes, "😀" = 4 bytes, " world" = 6 bytes → 16 bytes total
    let slice = s.substring(6, 16); // "😀 world"
    assert_eq!(slice.as_str(), "😀 world");
    assert_eq!(slice.char_count(), 7); // 😀 + " world" = 7 codepoints
    assert_eq!(slice.len(), 10); // 4 + 6 bytes
}

#[test]
fn char_count_concat() {
    let a = BexStr::from("hello");
    let b = BexStr::from("😀");
    let c = BexStr::concat(a, b);
    assert_eq!(c.char_count(), 6);
    assert_eq!(c.len(), 9);
}
