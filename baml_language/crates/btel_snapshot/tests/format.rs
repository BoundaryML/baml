//! Frozen v3 decoding compatibility and current writer round trips.
use std::{fmt::Write as _, sync::Arc};

use baml_type::{DeclarationName, RealizedTy, TaggedTypeName, TypeName, typetag::TypeTag};
use btel_snapshot::{
    BlobScratch, Description, Leaves, Limit, Limits, ShapePolicy, Shaper, Snapshot,
    SnapshotObject as O, SnapshotPool, SnapshotRoot, SnapshotValue as V, TypeIdentity,
};

fn hex(bytes: &[u8]) -> String {
    let mut output = String::new();
    for byte in bytes {
        write!(output, "{byte:02x}").unwrap();
    }
    output
}

fn graph(pool: &SnapshotPool, arguments: bool) -> Snapshot {
    let mut b = pool.try_acquire().unwrap();
    let name = DeclarationName::Declared(TypeName::local("Golden".into()));
    let declaration = b.declaration(&name, TypeTag::from_i64(42), true);
    let declaration = b.leaves().object(declaration).unwrap();
    let ty = b.leaves().ty(RealizedTy::Int);
    let text = b.leaves().string(&"value".into()).unwrap();
    let label = b.leaves().label(&"value".into()).unwrap();
    let field = |_: &mut Leaves<'_>, ()| ("field".into(), V::Int(7));
    let list = b.list(ty, [false, true].into_iter(), |_, flag| V::Bool(flag));
    let map = b.map(ty, ty, std::iter::once(()), |leaves, ()| {
        (leaves.string_value(&"field".into()), V::Int(7))
    });
    let instance = b.instance(declaration, [RealizedTy::Int], std::iter::once(()), field);
    let mut objects = vec![declaration];
    for object in [
        // Five bytes that a limit kept out of the capture.
        O::Uint8ArrayTruncated { original_len: 5 },
        list,
        map,
        instance,
        O::NonSnapshotableValue {},
    ] {
        objects.push(b.leaves().object(object).unwrap());
    }
    let cycle = b.leaves().reserve().unwrap();
    objects.push(cycle.id());
    let itself = V::Object(cycle.id());
    b.fill(cycle, O::Cell(itself));
    for (index, kind) in [
        Description::Function,
        Description::Closure,
        Description::BoundMethod,
        Description::GenericFunction,
        Description::HostFunction,
        Description::Future,
        Description::UnscheduledFuture,
        Description::Package,
        Description::Interface,
        Description::Implementation,
        Description::TypeAlias,
        Description::Sentinel,
    ]
    .into_iter()
    .enumerate()
    {
        let described = O::Descriptive {
            kind,
            name: (index % 2 == 0).then_some(label),
        };
        objects.push(b.leaves().object(described).unwrap());
    }
    let mut values = vec![
        V::Null,
        V::OmittedArg,
        V::Bool(true),
        V::Int(-7),
        V::Float(f64::from_bits(0x7ff8_0000_0000_1234)),
        V::String(text),
        V::Type(ty),
        V::Enum {
            declaration,
            variant: 2,
            name: label,
        },
    ];
    for integer in [-123, 0, 123] {
        values.push(b.leaves().bigint(&Arc::new(integer.into())));
    }
    for reason in [Limit::Values, Limit::Objects, Limit::Bytes, Limit::Depth] {
        objects.push(b.leaves().object(O::Truncated(reason)).unwrap());
        values.push(V::Truncated(reason));
    }
    values.extend(objects.into_iter().map(V::Object));
    let mut shaper = Shaper::default();
    if arguments {
        let mut root = b.arguments(values.into_iter(), |_, value| value);
        // As a limit leaves the arguments of a call that had one more.
        let SnapshotRoot::FunctionArgs(slots) = &mut root else {
            panic!("expected arguments")
        };
        slots.parameter_count += 1;
        b.finish(root, &mut shaper)
    } else {
        // The value root is a list of the same values.
        let everything = b.list(ty, values.into_iter(), |_, value| value);
        let everything = b.leaves().object(everything).unwrap();
        b.finish(V::Object(everything), &mut shaper)
    }
}

fn root_blob(snapshot: &Snapshot) -> Vec<u8> {
    assert_eq!(snapshot.blobs().len(), 1);
    let mut bytes = Vec::new();
    snapshot
        .root_blob()
        .write(&mut BlobScratch::default(), &mut bytes)
        .unwrap();
    bytes
}

#[test]
fn all_snapshot_tags_round_trip() {
    let pool = SnapshotPool::new(1, Limits::default());
    for arguments in [true, false] {
        let snapshot = graph(&pool, arguments);
        let bytes = root_blob(&snapshot);
        assert_eq!(&bytes[12..28], snapshot.root_id().as_bytes());
        let decoded = btel_snapshot::decode_blob(&bytes, &btel_snapshot::DecodeLimits::default())
            .expect("the reader must verify the writer's bytes");
        assert_eq!(decoded.id, snapshot.root_id());
        drop(snapshot);
        assert_eq!(pool.stats().in_use, 0);
    }
}

#[test]
#[ignore = "prints current encodings for review; never rewrites the fixtures"]
#[expect(clippy::print_stdout, reason = "the output is the point")]
fn print_current_encodings() {
    let pool = SnapshotPool::new(1, Limits::default());
    for arguments in [true, false] {
        println!("{arguments}: {}", hex(&root_blob(&graph(&pool, arguments))));
    }
}

#[test]
fn nominal_identity_tags_preserve_v3_bytes() {
    let tag = TypeTag::from_i64(42);
    let name = DeclarationName::Declared(TypeName::local("Golden".into()));
    assert_eq!(
        hex(&borsh::to_vec(&TypeIdentity::Unresolved(tag)).unwrap()),
        "002a00000000000000"
    );
    assert_eq!(
        hex(&borsh::to_vec(&TypeIdentity::Resolved(TaggedTypeName::new(tag, name))).unwrap()),
        "012a0000000000000000000000000006000000476f6c64656e"
    );
}

/// Arguments cut into blobs: a string and bytes stored alone, a map that
/// names a stored string, and two lists that hold each other, named once at
/// the root of their blob and once inside it.
fn cut_graph(pool: &SnapshotPool) -> Snapshot {
    let mut b = pool.try_acquire().unwrap();
    let ty = b.leaves().ty(RealizedTy::Int);
    let text = b.leaves().string_value(&"t".repeat(40).as_str().into());
    let note = b.leaves().string_value(&"n".repeat(40).as_str().into());
    let bytes = b.bytes(&[9; 40]);
    let array = b.leaves().object(bytes).unwrap();
    let entries = [
        ("note", note),
        ("padding", V::Int(1)),
        ("more padding", V::Int(2)),
    ];
    let map = b.map(ty, ty, entries.into_iter(), |leaves, (key, value)| {
        (leaves.string_value(&key.into()), value)
    });
    let map = b.leaves().object(map).unwrap();
    let [first_slot, second_slot] = [(); 2].map(|()| b.leaves().reserve().unwrap());
    let (first, second) = (first_slot.id(), second_slot.id());
    for (slot, other, label) in [
        (first_slot, second, "first"),
        (second_slot, first, "second"),
    ] {
        let label = b.leaves().string_value(&label.repeat(4).as_str().into());
        let list = b.list(ty, [V::Object(other), label].into_iter(), |_, item| item);
        b.fill(slot, list);
    }
    let slots = [
        text,
        V::Object(first),
        V::Object(map),
        V::Object(second),
        V::Object(array),
        text,
    ];
    let root = b.arguments(slots.into_iter(), |_, slot| slot);
    let mut shaper = Shaper::new(ShapePolicy::Split {
        unit_bytes: 64,
        leaf_bytes: 32,
    });
    b.finish(root, &mut shaper)
}

/// One line of hex per blob, children first.
fn blob_lines(snapshot: &Snapshot) -> Vec<String> {
    let mut scratch = BlobScratch::default();
    snapshot
        .blobs()
        .map(|blob| {
            let mut bytes = Vec::new();
            blob.write(&mut scratch, &mut bytes).unwrap();
            assert_eq!(blob.encoded_len(), bytes.len() as u64);
            let decoded =
                btel_snapshot::decode_blob(&bytes, &btel_snapshot::DecodeLimits::default())
                    .expect("the reader must verify the writer's bytes");
            assert_eq!(decoded.id, blob.id());
            hex(&bytes)
        })
        .collect()
}

#[test]
fn references_between_blobs_round_trip() {
    let pool = SnapshotPool::new(1, Limits::default());
    let lines = blob_lines(&cut_graph(&pool));
    assert!(lines.len() > 1);
    assert_eq!(pool.stats().in_use, 0);
}

#[test]
#[ignore = "prints current encodings for review; never rewrites the fixtures"]
#[expect(clippy::print_stdout, reason = "the output is the point")]
fn print_current_external_encodings() {
    let pool = SnapshotPool::new(1, Limits::default());
    for line in blob_lines(&cut_graph(&pool)) {
        println!("{line}");
    }
}

/// Media of every kind and source, with and without a MIME type and loaded
/// content, as a list root. The longest content is stored alone.
fn media_graph(pool: &SnapshotPool) -> Snapshot {
    use baml_type::MediaKind;
    use btel_snapshot::MediaSource;
    let mut b = pool.try_acquire().unwrap();
    let ty = b.leaves().ty(RealizedTy::Unknown);
    let mime = b.leaves().label(&"image/png".into()).unwrap();
    let url = b
        .leaves()
        .label(&"https://example.test/a.png".into())
        .unwrap();
    let path = b.leaves().label(&"/data/b.png".into()).unwrap();
    let short = b.leaves().string(&"iVBORw==".into()).unwrap();
    let long = b
        .leaves()
        .string(&"iVBORw0K".repeat(6).as_str().into())
        .unwrap();
    let sources = [
        (
            MediaKind::Image,
            Some(mime),
            MediaSource::Url { url, data: None },
        ),
        (
            MediaKind::Audio,
            None,
            MediaSource::Url {
                url,
                data: Some(short),
            },
        ),
        (
            MediaKind::Video,
            Some(mime),
            MediaSource::File { path, data: None },
        ),
        (
            MediaKind::Pdf,
            None,
            MediaSource::File {
                path,
                data: Some(long),
            },
        ),
        (
            MediaKind::Generic,
            Some(mime),
            MediaSource::Base64 { data: short },
        ),
        (MediaKind::Image, None, MediaSource::Base64 { data: long }),
    ];
    let list = b.list(
        ty,
        sources.into_iter(),
        |leaves, (kind, mime_type, source)| {
            let media = O::Media {
                kind,
                mime_type,
                source,
            };
            V::Object(leaves.object(media).unwrap())
        },
    );
    let list = b.leaves().object(list).unwrap();
    let mut shaper = Shaper::new(ShapePolicy::Split {
        unit_bytes: 1 << 20,
        leaf_bytes: 32,
    });
    b.finish(V::Object(list), &mut shaper)
}

#[test]
fn media_round_trips() {
    let pool = SnapshotPool::new(1, Limits::default());
    let lines = blob_lines(&media_graph(&pool));
    assert!(lines.len() > 1);
    assert_eq!(pool.stats().in_use, 0);
}

#[test]
#[ignore = "prints current encodings for review; never rewrites the fixtures"]
#[expect(clippy::print_stdout, reason = "the output is the point")]
fn print_current_media_encodings() {
    let pool = SnapshotPool::new(1, Limits::default());
    for line in blob_lines(&media_graph(&pool)) {
        println!("{line}");
    }
}

#[test]
fn frozen_v3_blobs_remain_readable_with_verified_content_ids() {
    for fixture in [
        include_str!("fixtures/format_v3_args.hex"),
        include_str!("fixtures/format_v3_value.hex"),
        include_str!("fixtures/format_v3_external.hex"),
        include_str!("fixtures/format_v3_media.hex"),
    ] {
        for line in fixture.lines() {
            let bytes: Vec<_> = line
                .as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect();
            let decoded =
                btel_snapshot::decode_blob(&bytes, &btel_snapshot::DecodeLimits::default())
                    .unwrap();
            assert_eq!(decoded.id.as_bytes(), &bytes[12..28]);
            for object in decoded.objects {
                if let btel_snapshot::DecodedObject::Map { entries, .. } = object {
                    assert!(
                        entries
                            .iter()
                            .all(|(key, _)| matches!(key, btel_snapshot::DecodedValue::String(_)))
                    );
                }
            }
        }
    }
}
