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

// ── Shared-Concat-node flatten regression (B-262 / B-233) ──────────────────
//
// Concatenation only defers into a `Concat` node when the result exceeds
// `INLINE_CAPACITY` (54 bytes); shorter results are eagerly copied to `Inline`
// and can't be shared. So these tests use >54-byte operands to force the
// `Concat` path, then check that flattening a *parent* does not corrupt a
// child node that is still referenced elsewhere.

/// A `Concat` reused as the left child of another `Concat` must keep its own
/// value after the parent is flattened. The old `flatten()` destructively
/// emptied shared inner nodes, so `child` would read back as `""`.
#[test]
fn shared_concat_child_survives_parent_flatten() {
    // 55 bytes → Concat (just over the 54-byte inline boundary).
    let child = BexStr::concat(BexStr::from("x".repeat(40)), BexStr::from("y".repeat(15)));
    assert!(
        matches!(child, BexStr::Concat(_)),
        "operand must be a Concat"
    );
    let expected_child = "x".repeat(40) + &"y".repeat(15);

    // Parent references `child` as its left operand (Arc clone → shared node).
    let parent = BexStr::concat(child.clone(), BexStr::from("#"));

    // Flatten the parent; this used to empty the shared `child` node.
    assert_eq!(parent.as_str(), format!("{expected_child}#"));

    // The original child must be intact — both by content and by equality.
    assert_eq!(child.as_str(), expected_child);
    assert_eq!(child.len(), 55);
    assert_eq!(child, BexStr::from(expected_child.as_str()));
}

/// Equality against a freshly-built literal after a shared flatten — mirrors
/// the `registry.baml` `t.name == full_name` lookup that produced
/// "Test not found".
#[test]
fn shared_concat_equality_after_parent_flatten() {
    let name = BexStr::concat(
        BexStr::from("testset_".repeat(4)),          // 32 bytes
        BexStr::from("/the_test_name_that_is_long"), // 27 bytes → 59 total
    );
    let expected = "testset_".repeat(4) + "/the_test_name_that_is_long";

    // hash_prefix = name + "#", then a `starts_with`-style flatten of it.
    let hash_prefix = BexStr::concat(name.clone(), BexStr::from("#"));
    let _ = hash_prefix.as_str(); // force flatten (what starts_with does)

    // name must still compare equal to its literal value.
    assert_eq!(name, BexStr::from(expected.as_str()));
    assert!(
        !name.as_str().is_empty(),
        "shared name was emptied by flatten"
    );
}

/// Two parents sharing one child: flattening the first must not corrupt the
/// child for the second.
#[test]
fn two_parents_share_one_concat_child() {
    let child = BexStr::concat(BexStr::from("a".repeat(50)), BexStr::from("b".repeat(10)));
    let expected = "a".repeat(50) + &"b".repeat(10);

    let p1 = BexStr::concat(child.clone(), BexStr::from("-1"));
    let p2 = BexStr::concat(child.clone(), BexStr::from("-2"));

    assert_eq!(p1.as_str(), format!("{expected}-1"));
    // Second parent and the child are still correct after p1 flattened.
    assert_eq!(p2.as_str(), format!("{expected}-2"));
    assert_eq!(child.as_str(), expected);
}

/// Nested sharing: a shared grandchild deep in the tree must also survive.
#[test]
fn shared_concat_grandchild_survives_flatten() {
    let leaf = BexStr::concat(BexStr::from("g".repeat(30)), BexStr::from("h".repeat(30)));
    let expected_leaf = "g".repeat(30) + &"h".repeat(30);

    let mid = BexStr::concat(leaf.clone(), BexStr::from("|mid"));
    let top = BexStr::concat(mid.clone(), BexStr::from("|top"));

    assert_eq!(top.as_str(), format!("{expected_leaf}|mid|top"));
    // Both the intermediate and the shared leaf survive.
    assert_eq!(mid.as_str(), format!("{expected_leaf}|mid"));
    assert_eq!(leaf.as_str(), expected_leaf);
}

// ── Codepoint-indexed ops vs the std oracle ────────────────────────────
//
// `byte_offset_of_nth_codepoint` walks the bytes 8 at a time (SWAR). The fast
// path is only exercised by strings with a full 8-byte word that *contains* a
// multibyte char AND a target index past that word — short ASCII fixtures stay
// in the scalar tail and never hit it. These cases all clear that bar, so they
// pin `char_at` / `substring` against `str::chars()`, which is correct by
// construction for valid UTF-8.
#[test]
fn codepoint_ops_match_std_oracle() {
    // The std reference: the byte range of codepoints `[a, b)`, with the same
    // clamping `substring_by_char` documents.
    fn expected_substring(s: &str, a: usize, b: usize) -> String {
        let chars: Vec<char> = s.chars().collect();
        let len = chars.len();
        let a = a.min(len);
        let b = b.min(len).max(a);
        chars[a..b].iter().collect()
    }

    let cases = [
        // The dogfood repro: `é` (C3 A9) sits inside the first 8-byte word, and
        // every index from 4 on lands past it. `char_at(10)` returned "b", and
        // `char_at(15)` (in bounds!) panicked OOB before the fix.
        "0123é56789abcdef",
        "abcdefghé0123456789",       // multibyte just after the first full word
        "配信サービスxyz0123456789", // 3-byte CJK spanning word boundaries
        "🪙abcdefghijklmnop",        // 4-byte leading codepoint
        "aé bé cé dé eé fé gé hé",   // many 2-byte chars across many words
        // Long enough to be a Flat (not Inline), mixed scripts.
        "mixed_файлов_مرحبا_🪙_padding_to_force_a_flat_allocation_here!!",
    ];

    for s in cases {
        let bex = BexStr::from(s);
        let count = s.chars().count();
        assert_eq!(bex.char_count(), count, "char_count for {s:?}");

        // char_at over every index, including the last (the OOB-panic boundary)
        // and one past the end (must be None).
        for n in 0..=count {
            let got = bex.char_at_codepoint(n).map(|c| c.as_str().to_owned());
            let want = s.chars().nth(n).map(|c| c.to_string());
            assert_eq!(got, want, "char_at({n}) for {s:?}");
        }

        // substring over every codepoint range, plus past-the-end clamps.
        for a in 0..=count + 1 {
            for b in a..=count + 1 {
                let got = bex.substring_by_char(a, b);
                assert_eq!(
                    got.as_str(),
                    expected_substring(s, a, b),
                    "substring_by_char({a}, {b}) for {s:?}"
                );
            }
        }
    }
}
