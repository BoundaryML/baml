//! Frozen v3 bytes, including blob IDs, covering every format tag.
//!
//! Regenerating these fixtures is a format change: review the printed bytes
//! from `print_format_v3_encodings` rather than copying them blindly.
use std::{fmt::Write as _, sync::Arc};

use baml_type::{DeclarationName, RealizedTy, TaggedTypeName, TypeName, typetag::TypeTag};
use btel_snapshot::{
    BlobScratch, Description, Limit, Limits, Shaper, Snapshot, SnapshotObject as O, SnapshotPool,
    SnapshotValue as V, TypeIdentity,
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
    b.set_object(
        declaration,
        O::Declaration {
            name: DeclarationName::Declared(TypeName::local("Golden".into())),
            tag: TypeTag::from_i64(42),
            is_enum: true,
        },
    );
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
    let bytes = b.copy_bytes(b"abc");
    let mut objects = vec![declaration];
    for object in [
        O::Uint8Array {
            data: bytes,
            original_len: 5,
        },
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
        let truncated = b.limited(reason);
        b.push_value(truncated);
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
