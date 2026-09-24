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
fn retained_heap_counts_complete_slice_parent() {
    assert_eq!(BexStr::from("inline").retained_heap_bytes(), Some(0));
    let parent = BexStr::from("x".repeat(100_000));
    let slice = parent.substring(1, 101);
    let bytes = parent.retained_heap_bytes().unwrap();
    assert!(bytes > parent.len());
    assert_eq!(slice.retained_heap_bytes(), Some(bytes));
    assert_eq!(slice.clone().retained_heap_bytes(), Some(bytes));
}

#[test]
fn retained_heap_flattens_ropes_to_stable_backing() {
    let parent = BexStr::from("x".repeat(100_000));
    let rope = BexStr::concat(parent.substring(0, 100), BexStr::from("y".repeat(100)));
    assert!(matches!(rope, BexStr::Concat(_)));
    let bytes = rope.retained_heap_bytes().unwrap();
    assert!(bytes > rope.len());
    assert!(bytes < parent.len());
    assert_eq!(rope.as_str().len(), 200);
    assert_eq!(rope.retained_heap_bytes(), Some(bytes));
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

// ── First / last occurrence, codepoint-indexed ─────────────────────────
//
// `char_index_of` and `char_last_index_of` search by *bytes* but report a
// *codepoint* index, so any multibyte text ahead of the match is where the two
// coordinate systems can diverge. Pinned against `str::find` / `str::rfind`
// plus a codepoint count, which is correct by construction for valid UTF-8.
#[test]
fn occurrence_index_ops_match_std_oracle() {
    let cases = [
        "hello world",
        "abcabc",
        "héllo",
        "🐑a🐑a🐑",
        "配信サービスxyz配信",
        "mixed_файлов_مرحبا_🪙_padding_to_force_a_flat_allocation_here!!",
    ];
    // Includes the empty needle (matches at both ends) and an absent one.
    let needles = ["", "a", "o", "é", "🐑", "配信", "bc", "zzz"];

    for s in cases {
        let bex = BexStr::from(s);
        for needle in needles {
            // `find`/`rfind` land on a char boundary, so slicing is valid.
            let want_first = s.find(needle).map(|b| s[..b].chars().count());
            let want_last = s.rfind(needle).map(|b| s[..b].chars().count());
            assert_eq!(
                bex.char_index_of(needle),
                want_first,
                "char_index_of({needle:?}) for {s:?}"
            );
            for start in 0..=s.chars().count() + 1 {
                let suffix: String = s.chars().skip(start).collect();
                let expected = if start > s.chars().count() {
                    None
                } else {
                    suffix
                        .find(needle)
                        .map(|b| start + suffix[..b].chars().count())
                };
                assert_eq!(
                    bex.char_index_of_from(needle, start),
                    expected,
                    "char_index_of_from({needle:?}, {start}) for {s:?}"
                );
            }
            assert_eq!(
                bex.char_index_of_from(needle, usize::MAX),
                None,
                "out-of-bounds search for {s:?}"
            );
            assert_eq!(
                bex.char_last_index_of(needle),
                want_last,
                "char_last_index_of({needle:?}) for {s:?}"
            );
        }
    }
}

#[test]
fn content_hash_is_independent_of_representation_and_reused_by_clones() {
    use crate::bex_str::ConcatState;
    let text = "a string longer than the inline capacity, with some unicode: λ🦀";
    let flat = BexStr::from(text);
    let rope = BexStr::concat(BexStr::from(&text[..20]), BexStr::from(&text[20..]));
    let BexStr::Concat(node) = &rope else {
        panic!("expected rope")
    };
    assert!(matches!(
        *node.state.lock().unwrap(),
        ConcatState::Deferred { .. }
    ));
    let expected = xxhash_rust::xxh3::xxh3_128(text.as_bytes());
    assert_eq!(rope.content_hash(), expected);
    assert!(matches!(
        *node.state.lock().unwrap(),
        ConcatState::Flattened(_)
    ));
    assert_eq!(flat.content_hash(), expected);
    let clone = flat.clone();
    let (BexStr::Flat(a), BexStr::Flat(b)) = (&flat, &clone) else {
        unreachable!()
    };
    assert!(std::sync::Arc::ptr_eq(a, b));
    assert_eq!(
        a.hash[1].load(std::sync::atomic::Ordering::Acquire),
        (expected >> 64) as u64
    );
    let padded = BexStr::from(format!("xx{text}yy"));
    let slice = padded.substring(2, 2 + text.len());
    assert_eq!(slice.content_hash(), expected);
    assert_eq!(slice.clone().content_hash(), expected);
    for s in ["", "short", "🦀"] {
        assert_eq!(
            BexStr::from(s).content_hash(),
            xxhash_rust::xxh3::xxh3_128(s.as_bytes())
        );
    }
}

#[test]
fn content_hash_can_be_published_concurrently() {
    let rope = BexStr::concat(
        BexStr::from("ab".repeat(256)),
        BexStr::from("cd".repeat(256)),
    );
    let expected =
        xxhash_rust::xxh3::xxh3_128(format!("{}{}", "ab".repeat(256), "cd".repeat(256)).as_bytes());
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let rope = &rope;
            scope.spawn(move || {
                for _ in 0..100 {
                    assert_eq!(rope.content_hash(), expected);
                }
            });
        }
    });
}
