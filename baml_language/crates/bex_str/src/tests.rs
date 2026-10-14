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
fn concat_handle_layout_uses_existing_payload() {
    use std::mem::size_of;

    use crate::bex_str::{ConcatNode, ConcatState, ConcatStr};

    assert_eq!(size_of::<BexStr>(), 56);
    assert_eq!(size_of::<ConcatStr>(), 48);
    assert_eq!(
        size_of::<ConcatNode>(),
        size_of::<std::sync::Mutex<ConcatState>>()
    );
    println!(
        "Concat handle={} node={} state={} Arc allocation={}",
        size_of::<ConcatStr>(),
        size_of::<ConcatNode>(),
        size_of::<ConcatState>(),
        size_of::<ConcatNode>() + 2 * size_of::<usize>()
    );
}

#[test]
fn concat_handle_suffix_reads_preserve_deferred_operands() {
    use crate::bex_str::ConcatState;

    let deferred = |s: &BexStr| {
        let BexStr::Concat(c) = s else {
            panic!("expected deferred concat")
        };
        assert!(matches!(
            *c.state.lock().unwrap(),
            ConcatState::Deferred { .. }
        ));
    };
    let right_rope = BexStr::concat(BexStr::from("界".repeat(20)), BexStr::from("\0é🙂x"));
    let flat = BexStr::from(format!("{}\0é🙂x!", "z".repeat(60)));
    let slice = flat.byte_slice(1..flat.byte_len() - 1);
    for right in [
        BexStr::from("\0"),
        BexStr::from("🙂"),
        BexStr::from("é中"),
        BexStr::from("x🙂\0"),
        BexStr::from("\0é🙂x"),
        flat,
        slice,
        right_rope.clone(),
    ] {
        let left = BexStr::concat(BexStr::from("é中🙂".repeat(20)), BexStr::from("prefix"));
        // Build the oracle from a separate clone only after checking that
        // construction and suffix reads do not materialize either operand.
        let s = BexStr::concat(left.clone(), right.clone());
        let count = s.char_len();
        let first = count - 4;
        let points: Vec<_> = (first..count).map(|n| s.char_at(n).unwrap()).collect();
        let tail: String = points.iter().collect();
        for start in first..=count + 1 {
            for end in [0, first, start, count - 1, count, usize::MAX] {
                let want: String = points
                    [start.min(count) - first..end.max(start).min(count) - first]
                    .iter()
                    .collect();
                assert_eq!(s.char_slice(start..end).as_str(), want);
            }
        }
        assert_eq!(s.char_at(count), None);
        assert_eq!(s.char_at(usize::MAX), None);
        assert_eq!(
            s.char_slice(std::ops::Range {
                start: usize::MAX,
                end: 0
            })
            .as_str(),
            ""
        );
        for n in first..=count {
            let suffix_bytes: usize = points[n - first..].iter().map(|c| c.len_utf8()).sum();
            assert_eq!(s.char_to_byte(n), s.byte_len() - suffix_bytes);
        }
        deferred(&s);
        deferred(&left);
        deferred(&right_rope);
        let want = format!("{}{}", left.as_str(), right.as_str());
        assert!(want.ends_with(&tail));
        assert_eq!(s.as_str(), want);
    }
}

#[test]
fn concat_handle_suffix_cache_preserves_shared_snapshots_and_publication() {
    use crate::bex_str::ConcatState;

    let mut text = "前é🙂".repeat(20);
    let mut s = BexStr::from(text.clone());
    let mut snapshots = Vec::new();
    for i in 0..128 {
        let part = ["\0", "🙂é", "中\0x", "🙂界é!"][i % 4];
        s = BexStr::concat(s.clone(), BexStr::from(part));
        text.push_str(part);
        let points: Vec<_> = text.chars().collect();
        let count = points.len();
        assert_eq!(s.byte_len(), text.len());
        assert_eq!(s.char_len(), count);
        assert_eq!(s.ascii_prefix_len(), 0);
        for (n, point) in points.iter().enumerate().skip(count - 4) {
            assert_eq!(s.char_at(n), Some(*point));
        }
        let BexStr::Concat(c) = &s else {
            panic!("expected concat")
        };
        assert!(matches!(
            *c.state.lock().unwrap(),
            ConcatState::Deferred { .. }
        ));
        if i % 17 == 0 {
            snapshots.push((s.clone(), text.clone()));
        }
    }
    let points: Vec<_> = text.chars().collect();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            assert_eq!(s.as_str(), text);
            assert_eq!(s.content_hash(), BexStr::from(text.clone()).content_hash());
        });
        for _ in 0..32 {
            for (n, point) in points.iter().enumerate().skip(points.len() - 4) {
                assert_eq!(s.char_at(n), Some(*point));
                assert_eq!(s.char_slice(n..n + 1).as_str(), point.to_string());
            }
        }
    });
    for (snapshot, want) in snapshots {
        assert_eq!(snapshot.as_str(), want);
        assert_eq!(snapshot.char_len(), want.chars().count());
        assert_eq!(snapshot.content_hash(), BexStr::from(want).content_hash());
    }
}

#[test]
fn retained_heap_counts_complete_slice_parent() {
    assert_eq!(BexStr::from("inline").retained_heap_bytes(), Some(0));
    let parent = BexStr::from("x".repeat(100_000));
    let slice = parent.byte_slice(1..101);
    let bytes = parent.retained_heap_bytes().unwrap();
    assert!(bytes > parent.byte_len());
    assert_eq!(slice.retained_heap_bytes(), Some(bytes));
    assert_eq!(slice.clone().retained_heap_bytes(), Some(bytes));
}

#[test]
fn unshared_heap_bytes_counts_each_handle_in_full() {
    assert_eq!(BexStr::from("inline").unshared_heap_bytes(), 0);

    let flat = BexStr::from("x".repeat(100_000));
    let bytes = flat.unshared_heap_bytes();
    assert!(bytes > flat.byte_len(), "a flat string owns its header too");
    assert_eq!(flat.clone().unshared_heap_bytes(), bytes);

    // A slice is sized by what it views, not by the parent it pins.
    assert_eq!(flat.byte_slice(1..1_001).unshared_heap_bytes(), 1_000);

    // A concatenation is sized by the buffer reading it would materialize,
    // and sizing it does not materialize that buffer.
    let rope = BexStr::concat(flat.clone(), BexStr::from("y".repeat(100)));
    assert!(matches!(rope, BexStr::Concat(_)));
    assert!(rope.unshared_heap_bytes() > rope.byte_len());
    assert!(matches!(
        &rope,
        BexStr::Concat(node) if matches!(
            &*node.state.lock().unwrap(),
            super::bex_str::ConcatState::Deferred { .. }
        )
    ));
}

#[test]
fn shared_heap_bytes_splits_a_buffer_among_its_handles() {
    assert_eq!(BexStr::from("inline").shared_heap_bytes(), 0);

    let flat = BexStr::from("x".repeat(100_000));
    let whole = flat.shared_heap_bytes();
    assert_eq!(whole, flat.unshared_heap_bytes());

    // Clones and slices all hold the same allocation, so their shares add up
    // to it (rounding each share up).
    let mut handles: Vec<BexStr> = (0..9)
        .map(|i| {
            if i % 2 == 0 {
                flat.clone()
            } else {
                flat.byte_slice(i..i + 500)
            }
        })
        .collect();
    handles.push(flat);
    let total: usize = handles.iter().map(BexStr::shared_heap_bytes).sum();
    assert!(total >= whole);
    assert!(total < whole + handles.len());
    // A small slice reports the same share as a full clone: it pins the parent.
    assert_eq!(
        handles[1].shared_heap_bytes(),
        handles[0].shared_heap_bytes()
    );

    // A shared concatenation node is split the same way, without flattening.
    let rope = BexStr::concat(BexStr::from("a".repeat(500)), BexStr::from("b".repeat(500)));
    let alone = rope.shared_heap_bytes();
    let other = rope.clone();
    assert_eq!(rope.shared_heap_bytes(), alone.div_ceil(2));
    assert_eq!(other.shared_heap_bytes(), alone.div_ceil(2));
}

#[test]
fn retained_heap_flattens_ropes_to_stable_backing() {
    let parent = BexStr::from("x".repeat(100_000));
    let rope = BexStr::concat(parent.byte_slice(0..100), BexStr::from("y".repeat(100)));
    assert!(matches!(rope, BexStr::Concat(_)));
    let bytes = rope.retained_heap_bytes().unwrap();
    assert!(bytes > rope.byte_len());
    assert!(bytes < parent.byte_len());
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
    let s1 = long.byte_slice(10..100);
    assert!(matches!(s1, BexStr::Slice { .. }));

    // Re-slice: still depth-1, same parent
    let s2 = s1.byte_slice(0..80);
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
    assert_eq!(e.byte_len(), 0);
    assert!(e.is_empty());
    assert_eq!(e.as_str(), "");
}

#[test]
fn from_empty_string() {
    let e = BexStr::from("");
    assert!(matches!(e, BexStr::Inline { byte_len: 0, .. }));
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
    let slice = flat.byte_slice(10..50); // Slice (40 bytes)
    let c = BexStr::concat(inline, slice);
    assert_eq!(c.byte_len(), 2 + 40);
    assert_eq!(&c.as_str()[..2], "hi");
    assert_eq!(&c.as_str()[2..], &"a".repeat(40));
}

#[test]
fn char_len_ascii() {
    let s = BexStr::from("hello");
    assert_eq!(s.char_len(), 5);
    assert_eq!(s.byte_len(), 5);
}

#[test]
fn char_len_multibyte() {
    let s = BexStr::from("héllo");
    assert_eq!(s.char_len(), 5);
    assert_eq!(s.byte_len(), 6); // é is 2 bytes
}

#[test]
fn char_len_emoji() {
    let s = BexStr::from("😀hello");
    assert_eq!(s.char_len(), 6);
    assert_eq!(s.byte_len(), 9); // 😀 is 4 bytes + "hello" is 5
}

#[test]
fn char_len_flat() {
    let s = BexStr::from("a".repeat(100) + "😀");
    assert_eq!(s.char_len(), 101);
    assert_eq!(s.byte_len(), 104);
}

#[test]
fn char_len_slice() {
    // substring takes BYTE offsets at the BexStr level
    let s = BexStr::from("hello 😀 world");
    // "hello " = 6 bytes, "😀" = 4 bytes, " world" = 6 bytes → 16 bytes total
    let slice = s.byte_slice(6..16); // "😀 world"
    assert_eq!(slice.as_str(), "😀 world");
    assert_eq!(slice.char_len(), 7); // 😀 + " world" = 7 codepoints
    assert_eq!(slice.byte_len(), 10); // 4 + 6 bytes
}

#[test]
fn char_len_concat() {
    let a = BexStr::from("hello");
    let b = BexStr::from("😀");
    let c = BexStr::concat(a, b);
    assert_eq!(c.char_len(), 6);
    assert_eq!(c.byte_len(), 9);
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
    assert_eq!(child.byte_len(), 55);
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
// `scan_char_to_byte` walks the bytes 8 at a time (SWAR). The fast
// path is only exercised by strings with a full 8-byte word that *contains* a
// multibyte char AND a target index past that word — short ASCII fixtures stay
// in the scalar tail and never hit it. These cases all clear that bar, so they
// pin `char_at` / `char_slice` against `str::chars()`, which is correct by
// construction for valid UTF-8.
#[test]
fn codepoint_ops_match_std_oracle() {
    // The std reference: the byte range of codepoints `[a, b)`, with the same
    // clamping `char_slice` documents.
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
        assert_eq!(bex.char_len(), count, "char_len for {s:?}");

        // char_at over every index, including the last (the OOB-panic boundary)
        // and one past the end (must be None).
        for n in 0..=count {
            let got = bex.char_at(n);
            let want = s.chars().nth(n);
            assert_eq!(got, want, "char_at({n}) for {s:?}");
        }

        // substring over every codepoint range, plus past-the-end clamps.
        for a in 0..=count + 1 {
            for b in a..=count + 1 {
                let got = bex.char_slice(a..b);
                assert_eq!(
                    got.as_str(),
                    expected_substring(s, a, b),
                    "char_slice({a}..{b}) for {s:?}"
                );
            }
        }
    }
}

// ── First / last occurrence, codepoint-indexed ─────────────────────────
//
// `char_find` and `char_rfind` search by *bytes* but report a
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
                bex.char_find(needle),
                want_first,
                "char_find({needle:?}) for {s:?}"
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
                    bex.char_find_from(needle, start),
                    expected,
                    "char_find_from({needle:?}, {start}) for {s:?}"
                );
            }
            assert_eq!(
                bex.char_find_from(needle, usize::MAX),
                None,
                "out-of-bounds search for {s:?}"
            );
            assert_eq!(
                bex.char_rfind(needle),
                want_last,
                "char_rfind({needle:?}) for {s:?}"
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
    let slice = padded.byte_slice(2..2 + text.len());
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

#[test]
fn char_len_of_deep_rope_does_not_recurse() {
    // `s = s + "é"` in a loop (and `Array.join`, which is such a loop) builds
    // a left-leaning rope as deep as the append count. Counting it must not
    // walk that depth on the call stack: run on a small stack, like a spawned
    // task, where a recursive walk overflowed at ~30k appends.
    std::thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(|| {
            let n = 200_000;
            let mut s = BexStr::from("x".repeat(60));
            for _ in 0..n {
                s = BexStr::concat(s, BexStr::from("é"));
            }
            assert!(matches!(s, BexStr::Concat(_)));
            assert_eq!(s.char_len(), 60 + n);
            assert_eq!(s.byte_len(), 60 + 2 * n);
            // Flattening keeps the cached count.
            assert_eq!(s.as_str().chars().count(), 60 + n);
            assert_eq!(s.char_len(), 60 + n);
        })
        .unwrap()
        .join()
        .unwrap();
}

// ── Codepoint indexing on long strings (B-1918) ────────────────────────
//
// `at` / `slice` / `index_of` on heap strings. The fixtures in
// `codepoint_ops_match_std_oracle` are short; these span many hundreds of
// codepoints with 1-, 2-, 3- and 4-byte chars, in every heap representation:
// Flat, Slice (starting mid-string, on a multibyte char), and Concat. Any
// indexing acceleration (ASCII fast path, offset index, cursor cache) must keep
// every lookup here identical to `str::chars()`.
#[test]
fn codepoint_ops_on_long_strings_match_std_oracle() {
    let unit = "ab é 配信 🪙 xyz ";
    let text = unit.repeat(40);
    let ascii = "0123456789abcdef".repeat(40);

    let flat = BexStr::from(text.as_str());
    let ascii_flat = BexStr::from(ascii.as_str());
    let slice_start = 9; // the start of `信`
    let slice_text = &text[slice_start..text.len() - 5];
    let slice = flat.byte_slice(slice_start..text.len() - 5);
    assert!(matches!(slice, BexStr::Slice { .. }));
    let ascii_slice_text = &ascii[3..ascii.len() - 3];
    let ascii_slice = ascii_flat.byte_slice(3..ascii.len() - 3);
    let mut concat = BexStr::empty();
    for _ in 0..40 {
        concat = BexStr::concat(concat, BexStr::from(unit));
    }
    assert!(matches!(concat, BexStr::Concat(_)));

    let cases: [(&str, &BexStr); 5] = [
        (&text, &flat),
        (&ascii, &ascii_flat),
        (slice_text, &slice),
        (ascii_slice_text, &ascii_slice),
        (&text, &concat),
    ];
    for (s, bex) in cases {
        let chars: Vec<char> = s.chars().collect();
        let count = chars.len();
        assert_eq!(bex.char_len(), count);
        let byte_of = |n: usize| s.char_indices().nth(n).map_or(s.len(), |(b, _)| b);

        for n in 0..=count + 1 {
            assert_eq!(bex.char_at(n), chars.get(n).copied(), "char_at({n})");
        }
        let mut bounds = vec![
            0,
            1,
            63,
            64,
            65,
            127,
            128,
            129,
            200,
            count - 1,
            count,
            count + 9,
        ];
        bounds.dedup();
        for &a in &bounds {
            for &b in &bounds {
                let (lo, hi) = (a.min(count), b.min(count).max(a.min(count)));
                let want: String = chars[lo..hi].iter().collect();
                assert_eq!(bex.char_slice(a..b).as_str(), want, "char_slice({a}..{b})");
            }
        }
        for needle in ["🪙", "配信 🪙 x", "é", "cdef0", "zzz"] {
            let want = s.find(needle).map(|b| s[..b].chars().count());
            assert_eq!(bex.char_find(needle), want, "char_find({needle:?})");
            let want_last = s.rfind(needle).map(|b| s[..b].chars().count());
            assert_eq!(bex.char_rfind(needle), want_last, "char_rfind({needle:?})");
            for start in [0, 63, 64, 65, 300, count, count + 1] {
                let want = (start <= count)
                    .then(|| {
                        let from = byte_of(start);
                        s[from..]
                            .find(needle)
                            .map(|r| s[..from + r].chars().count())
                    })
                    .flatten();
                assert_eq!(
                    bex.char_find_from(needle, start),
                    want,
                    "char_find_from({needle:?}, {start})"
                );
            }
        }
    }
}

// ── ASCII prefix ───────────────────────────────────────────────────────
//
// `ascii_prefix_len` is promised exact on every representation, so it is
// checked against the oracle (the first byte >= 0x80), along with the
// invariants that tie it to `byte_len` and `char_len`.
fn assert_ascii_prefix_exact(bex: &BexStr, what: &str) {
    let bytes = bex.as_str().as_bytes();
    let want = bytes
        .iter()
        .position(|b| !b.is_ascii())
        .unwrap_or(bytes.len());
    let p = bex.ascii_prefix_len();
    assert_eq!(p, want, "ascii_prefix_len of {what}");
    assert_eq!(
        bex.is_ascii(),
        bex.as_str().is_ascii(),
        "is_ascii of {what}"
    );
    assert!(
        p <= bex.char_len() && bex.char_len() <= bex.byte_len(),
        "{what}"
    );
    assert_eq!(
        p == bex.byte_len(),
        bex.char_len() == bex.byte_len(),
        "{what}"
    );
}

#[test]
fn ascii_prefix_matches_oracle_on_every_representation() {
    use crate::bex_str::ConcatState;
    let ascii = "0123456789abcdef".repeat(8); // 128 B
    let mixed = format!("{ascii}é{ascii}🪙{ascii}"); // non-ASCII at 128 and 258
    let leading = format!("é{ascii}"); // non-ASCII at 0

    let flat_ascii = BexStr::from(ascii.as_str());
    let flat_mixed = BexStr::from(mixed.as_str());
    let flat_leading = BexStr::from(leading.as_str());
    let mut cases: Vec<(String, BexStr)> = vec![
        ("empty".into(), BexStr::empty()),
        ("inline ascii".into(), BexStr::from("plain")),
        ("inline non-ascii".into(), BexStr::from("ab🪙cd")),
        ("inline leading non-ascii".into(), BexStr::from("é")),
        ("flat ascii".into(), flat_ascii.clone()),
        ("flat mixed".into(), flat_mixed.clone()),
        ("flat leading non-ascii".into(), flat_leading.clone()),
    ];
    // Slices of the mixed string with every start/end relationship to its
    // non-ASCII bytes at 128 and 258..262: inherited from the receiver (start
    // inside its prefix) or scanned (start at or past it).
    for (start, end) in [
        (0, 100),   // entirely in the prefix
        (10, 128),  // ends exactly at the first non-ASCII byte
        (10, 200),  // prefix runs into the é
        (128, 200), // starts on the é
        (130, 258), // between é and 🪙: ASCII, scanned
        (130, 300), // scanned, ends past 🪙
        (262, mixed.len()),
    ] {
        let slice = flat_mixed.byte_slice(start..end);
        assert!(matches!(slice, BexStr::Slice { .. }), "{start}..{end}");
        cases.push((format!("slice {start}..{end}"), slice));
    }
    // A slice of a slice inherits from the outer slice, not the root, which
    // starts with an é.
    let outer = flat_leading.byte_slice(2..leading.len());
    cases.push(("slice of leading".into(), outer.clone()));
    cases.push(("slice of slice".into(), outer.byte_slice(5..100)));
    // Ropes: ASCII + ASCII, ASCII + non-ASCII, non-ASCII + ASCII, nested, and
    // the same after flattening.
    let ropes = [
        (
            "rope ascii+ascii",
            BexStr::concat(flat_ascii.clone(), flat_ascii.clone()),
        ),
        (
            "rope ascii+mixed",
            BexStr::concat(flat_ascii.clone(), flat_mixed.clone()),
        ),
        (
            "rope mixed+ascii",
            BexStr::concat(flat_mixed.clone(), flat_ascii.clone()),
        ),
        (
            "rope ascii+leading",
            BexStr::concat(flat_ascii.clone(), flat_leading.clone()),
        ),
        (
            "rope (ascii+inline)+slice",
            BexStr::concat(
                BexStr::concat(flat_ascii.clone(), BexStr::from("tail")),
                flat_mixed.byte_slice(130..300),
            ),
        ),
    ];
    for (name, rope) in ropes {
        let BexStr::Concat(node) = &rope else {
            panic!("{name}: expected a rope")
        };
        assert!(matches!(
            *node.state.lock().unwrap(),
            ConcatState::Deferred { .. }
        ));
        // Checked before any flatten (cached on the node) ...
        assert_eq!(
            rope.ascii_prefix_len(),
            {
                let s = rope.clone();
                let bytes = s.as_str().as_bytes();
                bytes
                    .iter()
                    .position(|b| !b.is_ascii())
                    .unwrap_or(bytes.len())
            },
            "{name}"
        );
        // ... and after (`as_str` above flattened the shared node).
        cases.push((name.into(), rope.clone()));
        cases.push((
            format!("{name}, sliced"),
            rope.byte_slice(60..rope.byte_len() - 60),
        ));
    }
    for (what, bex) in &cases {
        assert_ascii_prefix_exact(bex, what);
    }
}

#[test]
fn ascii_prefix_of_deep_rope_does_not_recurse() {
    std::thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(|| {
            let n = 200_000;
            let mut s = BexStr::from("x".repeat(60));
            for _ in 0..n {
                s = BexStr::concat(s, BexStr::from("a"));
            }
            assert!(s.is_ascii());
            assert_eq!(s.ascii_prefix_len(), 60 + n);
            s = BexStr::concat(s, BexStr::from("🪙"));
            for _ in 0..n {
                s = BexStr::concat(s, BexStr::from("a"));
            }
            assert!(!s.is_ascii());
            assert_eq!(s.ascii_prefix_len(), 60 + n);
            // In the prefix, char indices resolve without flattening.
            assert_eq!(s.char_to_byte(60 + n), 60 + n);
            let BexStr::Concat(node) = &s else {
                panic!("expected a rope")
            };
            assert!(matches!(
                *node.state.lock().unwrap(),
                crate::bex_str::ConcatState::Deferred { .. }
            ));
            assert_eq!(s.char_to_byte(60 + n + 1), 60 + n + 4);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn slice_ascii_prefix_sentinel_rescans() {
    use std::sync::atomic::AtomicU64;

    use crate::bex_str::SLICE_ASCII_PREFIX_UNKNOWN;
    // A slice of 4 GiB or more stores the sentinel; build one by hand over a
    // small parent to check that reading it rescans to the exact value.
    let text = format!("{}é{}", "a".repeat(70), "b".repeat(30));
    let BexStr::Flat(parent) = BexStr::from(text.as_str()) else {
        panic!("expected a flat string")
    };
    let slice = BexStr::Slice {
        parent,
        offset: 5,
        byte_len: (text.len() - 10) as u64,
        char_len: (text.chars().count() - 10) as u64,
        ascii_prefix_len: SLICE_ASCII_PREFIX_UNKNOWN,
        hash: [const { AtomicU64::new(0) }; 2],
    };
    assert_eq!(slice.ascii_prefix_len(), 65);
    assert_ascii_prefix_exact(&slice, "sentinel slice");
    assert_ascii_prefix_exact(&slice.byte_slice(1..80), "slice of sentinel slice");
    assert_eq!(slice.char_slice(60..70).as_str(), &text[65..76]);
}

// `char_to_byte` / `byte_to_char` on both sides of the ASCII prefix, in every
// heap representation, against `str::char_indices`.
#[test]
fn char_byte_conversions_match_std_oracle() {
    let ascii = "0123456789abcdef".repeat(10);
    let unit = "ab é 配信 🪙 xyz ";
    let text = format!("{ascii}{}", unit.repeat(20));
    let flat = BexStr::from(text.as_str());
    let concat = BexStr::concat(BexStr::from(ascii.as_str()), BexStr::from(unit.repeat(20)));
    let slice = flat.byte_slice(40..text.len() - 3);
    let slice_text = &text[40..text.len() - 3];
    let mixed_slice = flat.byte_slice(ascii.len() + 3..text.len());
    let mixed_slice_text = &text[ascii.len() + 3..];
    let short = "ab é 配信 🪙 xyz";
    let inline = BexStr::from(short);
    assert!(matches!(inline, BexStr::Inline { .. }));
    let cases: [(&str, &BexStr); 5] = [
        (&text, &flat),
        (&text, &concat),
        (slice_text, &slice),
        (mixed_slice_text, &mixed_slice),
        (short, &inline),
    ];
    for (s, bex) in cases {
        let count = s.chars().count();
        let starts: Vec<usize> = s.char_indices().map(|(b, _)| b).chain([s.len()]).collect();
        for n in 0..=count + 2 {
            let want = starts.get(n).copied().unwrap_or(s.len());
            assert_eq!(bex.char_to_byte(n), want, "char_to_byte({n}) of {s:?}");
        }
        for (n, &b) in starts.iter().enumerate() {
            assert_eq!(bex.byte_to_char(b), n, "byte_to_char({b}) of {s:?}");
        }
        assert_eq!(bex.byte_to_char(s.len() + 5), count);
        for n in 0..=count {
            assert_eq!(bex.char_at(n), s.chars().nth(n), "char_at({n}) of {s:?}");
        }
    }
}

#[test]
fn char_slice_clamps_extreme_ranges_without_panicking() {
    let cases = [
        BexStr::from("ab é"),
        BexStr::from("ab é 配信 🪙 xyz ".repeat(10)),
        BexStr::concat(BexStr::from("a".repeat(60)), BexStr::from("é".repeat(60))),
    ];
    for bex in &cases {
        let n = bex.char_len();
        let s = bex.as_str().to_owned();
        let max = usize::MAX;
        assert_eq!(bex.char_slice(max..max).as_str(), "");
        assert_eq!(bex.char_slice(max - 1..max).as_str(), "");
        assert_eq!(bex.char_slice(n..max).as_str(), "");
        assert_eq!(bex.char_slice(0..max).as_str(), s);
        assert_eq!(
            bex.char_slice(n - 1..max).as_str(),
            s.chars().last().unwrap().to_string()
        );
        #[allow(clippy::reversed_empty_ranges)]
        let reversed = bex.char_slice(5..2);
        assert_eq!(reversed.as_str(), "");
        assert_eq!(bex.char_at(max), None);
        assert_eq!(bex.char_to_byte(max), bex.byte_len());
        assert_eq!(bex.char_find_from("a", max), None);
    }
}
