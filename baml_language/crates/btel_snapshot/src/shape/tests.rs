use baml_type::{DeclarationName, RealizedTy, TypeName, typetag::TypeTag};

use super::*;
use crate::{
    BlobScratch, Builder, DecodeLimits, DecodedObject, DecodedRoot, DecodedSnapshot, DecodedValue,
    Limits, NodeId, Snapshot, SnapshotObject as O, SnapshotPool, SnapshotValue as V, decode_blob,
};

fn split(unit_bytes: u64, leaf_bytes: u64) -> Shaper {
    Shaper::new(ShapePolicy::Split {
        unit_bytes,
        leaf_bytes,
    })
}

fn capture(shaper: &mut Shaper, build: impl FnOnce(&mut Builder) -> V) -> Snapshot {
    let pool = SnapshotPool::new(1, Limits::default());
    let mut b = pool.try_acquire().unwrap();
    let root = build(&mut b);
    b.finish_value(root, shaper)
}

fn text(b: &mut Builder, content: &str) -> V {
    V::String(b.string(&content.into()).unwrap())
}

fn list(b: &mut Builder, items: &[V]) -> V {
    let id = b.reserve_object().unwrap();
    fill(b, id, items);
    V::Object(id)
}

fn fill(b: &mut Builder, id: crate::ObjectId, items: &[V]) {
    let element_type = b.push_type(RealizedTy::Unknown);
    let start = b.value_start();
    for item in items {
        b.push_value(*item);
    }
    let items = b.value_range(start);
    b.set_object(
        id,
        O::List {
            element_type,
            items,
            original_len: items.len(),
        },
    );
}

fn map(b: &mut Builder, entries: &[(&str, V)]) -> V {
    let id = b.reserve_object().unwrap();
    let key_type = b.push_type(RealizedTy::String);
    let value_type = b.push_type(RealizedTy::Unknown);
    let start = b.entry_start();
    for (key, value) in entries {
        b.entry(&(*key).into(), *value);
    }
    let entries = b.entry_range(start);
    b.set_object(
        id,
        O::Map {
            key_type,
            value_type,
            entries,
            original_len: entries.len(),
        },
    );
    V::Object(id)
}

fn bytes(b: &mut Builder, content: &[u8]) -> V {
    let data = b.copy_bytes(content);
    let id = b.reserve_object().unwrap();
    b.set_object(
        id,
        O::Uint8Array {
            data,
            original_len: content.len(),
        },
    );
    V::Object(id)
}

/// Every blob's bytes, children first, each checked against what shaping
/// recorded and against the reader.
fn written(snapshot: &Snapshot) -> Vec<(Vec<u8>, DecodedSnapshot)> {
    let mut scratch = BlobScratch::default();
    let mut seen = Vec::new();
    snapshot
        .blobs()
        .map(|blob| {
            let mut encoded = Vec::new();
            blob.write(&mut scratch, &mut encoded).unwrap();
            assert_eq!(blob.encoded_len(), encoded.len() as u64);
            let decoded = decode_blob(&encoded, &DecodeLimits::default())
                .unwrap_or_else(|error| panic!("blob {:?}: {error}", blob.index()));
            assert_eq!(decoded.id, blob.id());
            let children: Vec<_> = blob.children().collect();
            assert_eq!(
                decoded.children,
                children.iter().map(crate::Blob::id).collect::<Vec<_>>()
            );
            for child in &children {
                assert!(child.index() < blob.index(), "children come first");
            }
            assert!(!seen.contains(&blob.id()), "a blob is stored once");
            seen.push(blob.id());
            (encoded, decoded)
        })
        .collect()
}

fn items(blob: &DecodedSnapshot, node: u32) -> &[DecodedValue] {
    match blob.object(NodeId(node)) {
        DecodedObject::List { items, .. } => items,
        other => panic!("expected a list, found {other:?}"),
    }
}

#[test]
fn the_default_policy_keeps_a_capture_whole() {
    assert_eq!(Shaper::default().policy(), ShapePolicy::Whole);
    let snapshot = capture(&mut Shaper::default(), |b| {
        let large = text(b, &"x".repeat(1 << 20));
        list(b, &[large, large])
    });
    assert_eq!(written(&snapshot).len(), 1);
}

/// A map over the unit threshold that holds a string over the leaf threshold.
fn order(b: &mut Builder) -> V {
    let note = text(b, &"n".repeat(300));
    let sku = text(b, &"s".repeat(60));
    map(b, &[("note", note), ("sku", sku), ("quantity", V::Int(3))])
}

#[test]
fn a_cut_value_is_the_blob_that_capturing_it_alone_produces() {
    let mut shaper = split(64, 256);
    let whole = capture(&mut shaper, |b| {
        let order = order(b);
        let small = text(b, "small");
        list(b, &[small, order, V::Int(7)])
    });
    let blobs = written(&whole);
    // The note, the order that names it, and the list that names the order.
    assert_eq!(blobs.len(), 3);
    let root = &blobs[2].1;
    assert_eq!(
        items(root, 0),
        [
            DecodedValue::String("small".into()),
            DecodedValue::External(crate::ChildIndex(0)),
            DecodedValue::Int(7),
        ]
    );
    assert_eq!(root.children, [blobs[1].1.id]);
    assert_eq!(blobs[1].1.children, [blobs[0].1.id]);
    assert_eq!(
        blobs[0].1.root,
        DecodedRoot::Value(DecodedValue::String("n".repeat(300).into()))
    );

    let alone = written(&capture(&mut shaper, order));
    assert_eq!(alone.len(), 2);
    assert_eq!(alone[0].0, blobs[0].0);
    assert_eq!(alone[1].0, blobs[1].0);
    let note = written(&capture(&mut shaper, |b| text(b, &"n".repeat(300))));
    assert_eq!(note.len(), 1);
    assert_eq!(note[0].0, blobs[0].0);
}

/// Two lists that hold each other, each with text of its own.
fn ring(b: &mut Builder) -> (V, V) {
    let first = b.reserve_object().unwrap();
    let second = b.reserve_object().unwrap();
    let text_first = text(b, &"a".repeat(30));
    let text_second = text(b, &"b".repeat(30));
    fill(b, first, &[V::Object(second), text_first]);
    fill(b, second, &[V::Object(first), text_second]);
    (V::Object(first), V::Object(second))
}

#[test]
fn a_cycle_is_one_blob_and_other_blobs_name_its_members() {
    let mut shaper = split(64, 1024);
    let holder = |swap: bool, shaper: &mut Shaper| {
        capture(shaper, |b| {
            let (first, second) = ring(b);
            if swap {
                list(b, &[second, first])
            } else {
                list(b, &[first, second])
            }
        })
    };
    let blobs = written(&holder(false, &mut shaper));
    assert_eq!(blobs.len(), 2);
    let (cycle, root) = (&blobs[0].1, &blobs[1].1);
    assert_eq!(cycle.objects.len(), 2);
    assert_eq!(
        cycle.root,
        DecodedRoot::Value(DecodedValue::Object(NodeId(0)))
    );
    // Each member names the other by its number in the shared blob.
    assert_eq!(items(cycle, 0)[0], DecodedValue::Object(NodeId(1)));
    assert_eq!(items(cycle, 1)[0], DecodedValue::Object(NodeId(0)));
    let starts_at_first = items(cycle, 0)[1] == DecodedValue::String("a".repeat(30).into());
    let child = crate::ChildIndex(0);
    let at_root = DecodedValue::External(child);
    let inside = DecodedValue::ExternalNode {
        child,
        node: NodeId(1),
    };
    let expected = if starts_at_first {
        [at_root, inside]
    } else {
        [inside, at_root]
    };
    assert_eq!(items(root, 0), expected);

    // The cycle's blob does not depend on which member is met first.
    let swapped = written(&holder(true, &mut shaper));
    assert_eq!(swapped[0].0, blobs[0].0);
    assert_eq!(
        items(&swapped[1].1, 0),
        [expected[1].clone(), expected[0].clone()]
    );

    // A capture of one member is a single blob that starts at that member.
    let from = |second: bool, shaper: &mut Shaper| {
        written(&capture(shaper, |b| {
            let (first, other) = ring(b);
            if second { other } else { first }
        }))
    };
    let (first, second) = (from(false, &mut shaper), from(true, &mut shaper));
    assert_eq!((first.len(), second.len()), (1, 1));
    assert_ne!(first[0].0, second[0].0);
    let canonical = if starts_at_first { &first } else { &second };
    assert_eq!(canonical[0].0, blobs[0].0);
}

#[test]
fn a_chain_is_cut_into_blobs_of_bounded_size() {
    let links = 200;
    let snapshot = capture(&mut split(256, 1024), |b| {
        let mut next = None;
        for link in (0..links).rev() {
            let label = text(b, &format!("link {link:>15}"));
            next = Some(match next {
                Some(next) => list(b, &[label, next]),
                None => list(b, &[label]),
            });
        }
        next.unwrap()
    });
    let blobs = written(&snapshot);
    assert!(blobs.len() > 20, "{} blobs", blobs.len());
    let mut objects = 0;
    for (encoded, decoded) in &blobs {
        // A blob ends at the first link that brings it to the threshold.
        assert!(encoded.len() < 256 + 128, "{} bytes", encoded.len());
        assert!(decoded.children.len() <= 1);
        objects += decoded.objects.len();
    }
    assert_eq!(objects, links);
}

#[test]
fn a_million_objects_in_a_chain_shape_on_a_small_stack() {
    let blobs = std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(|| {
            let pool = SnapshotPool::new(1, Limits::default());
            let mut b = pool.try_acquire().unwrap();
            let mut next = V::Null;
            for _ in 0..1_000_000 {
                let id = b.reserve_object().unwrap();
                b.set_object(id, O::Cell(next));
                next = V::Object(id);
            }
            let snapshot = b.finish_value(next, &mut split(64 << 10, 16 << 10));
            let mut scratch = BlobScratch::default();
            let mut total = 0;
            for blob in snapshot.blobs() {
                let mut counter = Counter(0);
                blob.write(&mut scratch, &mut counter).unwrap();
                assert_eq!(counter.0, blob.encoded_len());
                total += counter.0;
            }
            assert!(total < 8 << 20, "{total} bytes");
            snapshot.blobs().len()
        })
        .unwrap()
        .join()
        .unwrap();
    assert!(blobs > 50, "{blobs} blobs");
}

struct Counter(u64);
impl std::io::Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len() as u64;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn values_reached_many_ways_are_stored_once_per_blob() {
    // Each level holds the next twice: forty levels reach the last one along
    // 2^40 paths.
    let levels = 40;
    let snapshot = capture(&mut split(1024, 4096), |b| {
        let mut next = text(b, &"leaf".repeat(25));
        for _ in 0..levels {
            next = list(b, &[next, next]);
        }
        next
    });
    let blobs = written(&snapshot);
    let total: usize = blobs.iter().map(|(encoded, _)| encoded.len()).sum();
    let objects: usize = blobs.iter().map(|(_, blob)| blob.objects.len()).sum();
    assert_eq!(objects, levels, "each level is stored in one blob");
    assert!(blobs.len() > 1 && blobs.len() <= levels);
    assert!(total < 8 << 10, "{total} bytes");
}

#[test]
fn keys_names_and_declarations_stay_in_the_blob_that_uses_them() {
    let long = "k".repeat(200);
    let snapshot = capture(&mut split(1 << 20, 64), |b| {
        let declaration = b.reserve_object().unwrap();
        b.set_object(
            declaration,
            O::Declaration {
                name: DeclarationName::Declared(TypeName::from_dotted_path(&format!(
                    "user.{long}"
                ))),
                tag: TypeTag::from_i64(7),
                is_enum: true,
            },
        );
        let name = b.string(&long.as_str().into()).unwrap();
        let function = b.reserve_object().unwrap();
        b.set_object(
            function,
            O::Descriptive {
                kind: crate::Description::Function,
                name: Some(name),
            },
        );
        let value = V::String(name);
        let variant = V::Enum {
            declaration,
            variant: 0,
            name,
        };
        map(
            b,
            &[
                (&long, value),
                ("variant", variant),
                ("function", V::Object(function)),
            ],
        )
    });
    let blobs = written(&snapshot);
    // Only the string held as a value has a blob of its own.
    assert_eq!(blobs.len(), 2);
    assert_eq!(
        blobs[0].1.root,
        DecodedRoot::Value(DecodedValue::String(long.as_str().into()))
    );
    let root = &blobs[1].1;
    let DecodedObject::Map { entries, .. } = root.object(NodeId(0)) else {
        panic!("expected a map")
    };
    assert_eq!(&*entries[0].0, long);
    assert_eq!(entries[0].1, DecodedValue::External(crate::ChildIndex(0)));
    let DecodedValue::Enum {
        declaration, name, ..
    } = &entries[1].1
    else {
        panic!("expected an enum value")
    };
    assert_eq!(&**name, long);
    assert!(matches!(
        root.object(*declaration),
        DecodedObject::Declaration { .. }
    ));
    let DecodedValue::Object(function) = entries[2].1 else {
        panic!("expected an object")
    };
    assert_eq!(
        root.object(function),
        &DecodedObject::Descriptive {
            kind: crate::Description::Function,
            name: Some(long.as_str().into()),
        }
    );
}

#[test]
fn equal_values_share_one_blob_and_one_child_slot() {
    let snapshot = capture(&mut split(1 << 20, 64), |b| {
        let payload = [7_u8; 100];
        let strings = [(); 2].map(|()| text(b, &"same".repeat(40)));
        let arrays = [(); 2].map(|()| bytes(b, &payload));
        let small = [(); 2].map(|()| bytes(b, &[1, 2, 3]));
        list(
            b,
            &[
                strings[0], arrays[0], strings[1], arrays[1], small[0], small[1],
            ],
        )
    });
    let blobs = written(&snapshot);
    assert_eq!(blobs.len(), 3);
    let root = &blobs[2].1;
    let (string, array) = (crate::ChildIndex(0), crate::ChildIndex(1));
    assert_eq!(
        items(root, 0),
        [
            DecodedValue::External(string),
            DecodedValue::External(array),
            DecodedValue::External(string),
            DecodedValue::External(array),
            // Below the threshold, each array keeps its own identity.
            DecodedValue::Object(NodeId(1)),
            DecodedValue::Object(NodeId(2)),
        ]
    );
    let (_, stored) = blobs
        .iter()
        .find(|(_, blob)| blob.id == root.child(array))
        .unwrap();
    assert!(matches!(
        stored.object(NodeId(0)),
        DecodedObject::Uint8Array { data, .. } if data == &[7_u8; 100]
    ));
}

#[test]
fn argument_slots_name_cut_values_and_scratch_is_clean_between_captures() {
    let mut shaper = split(64, 64);
    let pool = SnapshotPool::new(1, Limits::default());
    let arguments = |shaper: &mut Shaper| {
        let mut b = pool.try_acquire().unwrap();
        let large = text(&mut b, &"argument".repeat(20));
        let order = order(&mut b);
        let start = b.value_start();
        for value in [large, V::OmittedArg, order, large] {
            b.push_value(value);
        }
        let slots = b.value_range(start);
        b.finish_args(4, slots, shaper)
    };
    let first = written(&arguments(&mut shaper));
    // Another capture in between must leave nothing behind.
    written(&capture(&mut shaper, |b| {
        let (first, second) = ring(b);
        list(b, &[first, second])
    }));
    let second = written(&arguments(&mut shaper));
    assert_eq!(first, second);
    let root = &first.last().unwrap().1;
    let DecodedRoot::FunctionArgs {
        parameter_count,
        slots,
    } = &root.root
    else {
        panic!("expected arguments")
    };
    assert_eq!(*parameter_count, 4);
    assert_eq!(slots[0], DecodedValue::External(crate::ChildIndex(0)));
    assert_eq!(slots[1], DecodedValue::OmittedArg);
    assert_eq!(slots[2], DecodedValue::External(crate::ChildIndex(1)));
    assert_eq!(slots[3], slots[0]);
    assert!(root.objects.is_empty());
}
