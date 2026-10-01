//! Frozen v3 bytes, including blob IDs, covering every format tag.
//!
//! Regenerating these fixtures is a format change: review the printed bytes
//! from `print_format_v3_encodings` rather than copying them blindly.
use std::{fmt::Write as _, sync::Arc};

use baml_type::{DeclarationName, RealizedTy, TaggedTypeName, TypeName, typetag::TypeTag};
use btel_snapshot::{
    BlobScratch, Description, Limit, Limits, ShapePolicy, Shaper, Snapshot, SnapshotObject as O,
    SnapshotPool, SnapshotValue as V, TypeIdentity,
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
    let declaration = b.reserve_object().unwrap();
    let name = DeclarationName::Declared(TypeName::local("Golden".into()));
    let object = b.declaration(&name, TypeTag::from_i64(42), true);
    b.set_object(declaration, object);
    let type_start = b.type_start();
    let ty = b.push_type(RealizedTy::Int);
    let type_arguments = b.type_range(type_start);
    let text = b.string(&"value".into()).unwrap();
    let entries_start = b.entry_start();
    b.entry(&"field".into(), V::Int(7));
    let entries = b.entry_range(entries_start);
    let value_start = b.value_start();
    b.push_value(V::Bool(false));
    b.push_value(V::Bool(true));
    let items = b.value_range(value_start);
    let mut objects = vec![declaration];
    for object in [
        // Five bytes that a limit kept out of the capture.
        O::Uint8ArrayTruncated { original_len: 5 },
        O::List {
            element_type: ty,
            items,
            original_len: 2,
        },
        O::Map {
            key_type: ty,
            value_type: ty,
            entries,
            original_len: 1,
        },
        O::Instance {
            type_arguments,
            declaration,
            fields: entries,
            original_len: 1,
        },
        O::NonSnapshotableValue {},
    ] {
        let id = b.reserve_object().unwrap();
        b.set_object(id, object);
        objects.push(id);
    }
    let cycle = b.reserve_object().unwrap();
    b.set_object(cycle, O::Cell(V::Object(cycle)));
    objects.push(cycle);
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
        let id = b.reserve_object().unwrap();
        b.set_object(
            id,
            O::Descriptive {
                kind,
                name: (index % 2 == 0).then_some(text),
            },
        );
        objects.push(id);
    }
    let root_start = b.value_start();
    for value in [
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
            name: text,
        },
    ] {
        b.push_value(value);
    }
    for integer in [-123, 0, 123] {
        let id = b.bigint(&Arc::new(integer.into())).unwrap();
        b.push_value(V::Bigint(id));
    }
    for reason in [Limit::Values, Limit::Objects, Limit::Bytes, Limit::Depth] {
        let id = b.reserve_object().unwrap();
        b.set_object(id, O::Truncated(reason));
        objects.push(id);
        b.push_value(V::Truncated(reason));
    }
    for object in objects {
        b.push_value(V::Object(object));
    }
    let slots = b.value_range(root_start);
    // The value root is a list of the same values. Unreachable from the
    // argument root, it is not part of that blob.
    let everything = b.reserve_object().unwrap();
    b.set_object(
        everything,
        O::List {
            element_type: ty,
            items: slots,
            original_len: slots.len(),
        },
    );
    if arguments {
        b.finish_args(slots.len() + 1, slots, &mut Shaper::default())
    } else {
        b.finish_value(V::Object(everything), &mut Shaper::default())
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
fn all_snapshot_tags_preserve_v3_bytes_and_content_ids() {
    let pool = SnapshotPool::new(1, Limits::default());
    for (arguments, expected) in [
        (true, include_str!("fixtures/format_v3_args.hex")),
        (false, include_str!("fixtures/format_v3_value.hex")),
    ] {
        let snapshot = graph(&pool, arguments);
        let bytes = root_blob(&snapshot);
        assert_eq!(hex(&bytes), expected.trim());
        assert_eq!(&bytes[12..28], snapshot.root_id().as_bytes());
        let decoded = btel_snapshot::decode_blob(&bytes, &btel_snapshot::DecodeLimits::default())
            .expect("the reader must verify the frozen v3 fixture");
        assert_eq!(decoded.id, snapshot.root_id());
        drop(snapshot);
        assert_eq!(pool.stats().in_use, 0);
    }
}

#[test]
#[ignore = "prints current encodings for review; never rewrites the fixtures"]
#[expect(clippy::print_stdout, reason = "the output is the point")]
fn print_format_v3_encodings() {
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
    let ty = b.push_type(RealizedTy::Int);
    let text = b.string(&"t".repeat(40).as_str().into()).unwrap();
    let note = b.string(&"n".repeat(40).as_str().into()).unwrap();
    let bytes = b.bytes(&[9; 40]);
    let array = b.reserve_object().unwrap();
    b.set_object(array, bytes);
    let map = b.reserve_object().unwrap();
    let entries_start = b.entry_start();
    b.entry(&"note".into(), V::String(note));
    b.entry(&"padding".into(), V::Int(1));
    b.entry(&"more padding".into(), V::Int(2));
    let entries = b.entry_range(entries_start);
    b.set_object(
        map,
        O::Map {
            key_type: ty,
            value_type: ty,
            entries,
            original_len: 3,
        },
    );
    let [first, second] = [(); 2].map(|()| b.reserve_object().unwrap());
    for (list, other, label) in [(first, second, "first"), (second, first, "second")] {
        let label = b.string(&label.repeat(4).as_str().into()).unwrap();
        let start = b.value_start();
        b.push_value(V::Object(other));
        b.push_value(V::String(label));
        let items = b.value_range(start);
        b.set_object(
            list,
            O::List {
                element_type: ty,
                items,
                original_len: 2,
            },
        );
    }
    let start = b.value_start();
    for value in [
        V::String(text),
        V::Object(first),
        V::Object(map),
        V::Object(second),
        V::Object(array),
        V::String(text),
    ] {
        b.push_value(value);
    }
    let slots = b.value_range(start);
    let mut shaper = Shaper::new(ShapePolicy::Split {
        unit_bytes: 64,
        leaf_bytes: 32,
    });
    b.finish_args(6, slots, &mut shaper)
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
                    .expect("the reader must verify the frozen v3 fixture");
            assert_eq!(decoded.id, blob.id());
            hex(&bytes)
        })
        .collect()
}

#[test]
fn references_between_blobs_preserve_v3_bytes_and_content_ids() {
    let pool = SnapshotPool::new(1, Limits::default());
    let lines = blob_lines(&cut_graph(&pool));
    let expected: Vec<_> = include_str!("fixtures/format_v3_external.hex")
        .lines()
        .collect();
    assert_eq!(lines, expected);
    assert_eq!(pool.stats().in_use, 0);
}

#[test]
#[ignore = "prints current encodings for review; never rewrites the fixtures"]
#[expect(clippy::print_stdout, reason = "the output is the point")]
fn print_format_v3_external_encodings() {
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
    let ty = b.push_type(RealizedTy::Unknown);
    let mime = b.string(&"image/png".into()).unwrap();
    let url = b.string(&"https://example.test/a.png".into()).unwrap();
    let path = b.string(&"/data/b.png".into()).unwrap();
    let short = b.string(&"iVBORw==".into()).unwrap();
    let long = b.string(&"iVBORw0K".repeat(6).as_str().into()).unwrap();
    let start = b.value_start();
    for (kind, mime_type, source) in [
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
    ] {
        let id = b.reserve_object().unwrap();
        b.set_object(
            id,
            O::Media {
                kind,
                mime_type,
                source,
            },
        );
        b.push_value(V::Object(id));
    }
    let items = b.value_range(start);
    let list = b.reserve_object().unwrap();
    b.set_object(
        list,
        O::List {
            element_type: ty,
            items,
            original_len: items.len(),
        },
    );
    let mut shaper = Shaper::new(ShapePolicy::Split {
        unit_bytes: 1 << 20,
        leaf_bytes: 32,
    });
    b.finish_value(V::Object(list), &mut shaper)
}

#[test]
fn media_preserves_v3_bytes_and_content_ids() {
    let pool = SnapshotPool::new(1, Limits::default());
    let lines = blob_lines(&media_graph(&pool));
    let expected: Vec<_> = include_str!("fixtures/format_v3_media.hex")
        .lines()
        .collect();
    assert_eq!(lines, expected);
    assert_eq!(pool.stats().in_use, 0);
}

#[test]
#[ignore = "prints current encodings for review; never rewrites the fixtures"]
#[expect(clippy::print_stdout, reason = "the output is the point")]
fn print_format_v3_media_encodings() {
    let pool = SnapshotPool::new(1, Limits::default());
    for line in blob_lines(&media_graph(&pool)) {
        println!("{line}");
    }
}
