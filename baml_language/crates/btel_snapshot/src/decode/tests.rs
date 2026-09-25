use std::sync::Arc;

use baml_type::{DeclarationName, RealizedTy, TypeName, typetag::TypeTag};
use num_bigint::BigInt;

use super::*;
use crate::{Limits, MapEntry, Snapshot, SnapshotObject as O, SnapshotPool, SnapshotValue as V};

fn int_type() -> OwnedType {
    RealizedTy::Int
}
fn customer() -> DeclarationName {
    DeclarationName::Declared(TypeName::from_dotted_path("user.Customer"))
}
fn encode(snapshot: &Snapshot) -> Vec<u8> {
    let mut bytes = Vec::new();
    snapshot.write_blob(&mut bytes).unwrap();
    bytes
}
fn decode(bytes: &[u8]) -> Result<DecodedSnapshot, BlobError> {
    decode_blob(bytes, &DecodeLimits::default())
}

/// Instance(Customer){name, tags: List[shared list, self-cycle cell], ...}
/// covering every value and object kind the capture code produces.
fn rich_snapshot(pool: &SnapshotPool) -> Snapshot {
    let mut b = pool.try_acquire().unwrap();
    let declaration = b.reserve_object().unwrap();
    b.set_object(
        declaration,
        O::Declaration {
            name: customer(),
            tag: TypeTag::from_i64(42),
            is_enum: false,
        },
    );
    let list = b.reserve_object().unwrap();
    let cell = b.reserve_object().unwrap();
    let map = b.reserve_object().unwrap();
    let bytes = b.reserve_object().unwrap();
    let instance = b.reserve_object().unwrap();
    let function = b.reserve_object().unwrap();
    let opaque = b.reserve_object().unwrap();
    let truncated = b.reserve_object().unwrap();

    let element_type = b.push_type(RealizedTy::List(Box::new(int_type())));
    let start = b.value_start();
    for v in [
        V::Int(-7),
        V::Float(f64::from_bits(0x7ff8_0000_0000_1234)),
        V::Bool(true),
    ] {
        b.push_value(v);
    }
    // The list contains itself: a cycle through a container.
    b.push_value(V::Object(list));
    let items = b.value_range(start);
    b.set_object(
        list,
        O::List {
            element_type,
            items,
            original_len: 9,
        },
    );
    b.set_object(cell, O::Cell(V::Object(cell)));

    let key = b.push_type(RealizedTy::String);
    let value = b.push_type(int_type());
    let text = b.string(&"hello λ".into()).unwrap();
    let start = b.entry_start();
    b.entry(&"greeting".into(), V::String(text));
    b.entry(&"list".into(), V::Object(list));
    let entries = b.entry_range(start);
    b.set_object(
        map,
        O::Map {
            key_type: key,
            value_type: value,
            entries,
            original_len: 2,
        },
    );
    let data = b.copy_bytes(&[1, 2, 3]);
    b.set_object(
        bytes,
        O::Bytes {
            data,
            original_len: 3,
        },
    );
    let types = b.type_start();
    b.push_type(int_type());
    let type_arguments = b.type_range(types);
    let big = b.bigint(&Arc::new(BigInt::from(-1) << 100)).unwrap();
    let variant = b.string(&"Active".into()).unwrap();
    let ty_value = b.push_type(int_type());
    let start = b.entry_start();
    b.entry(&"name".into(), V::String(text));
    b.entry(&"map".into(), V::Object(map));
    b.entry(&"shared".into(), V::Object(list));
    b.entry(&"bytes".into(), V::Object(bytes));
    b.entry(&"big".into(), V::Bigint(big));
    b.entry(&"null".into(), V::Null);
    b.entry(
        &"status".into(),
        V::Enum {
            declaration,
            variant: 1,
            name: variant,
        },
    );
    b.entry(&"type".into(), V::Type(ty_value));
    b.entry(&"cell".into(), V::Object(cell));
    b.entry(&"function".into(), V::Object(function));
    b.entry(&"host".into(), V::Object(opaque));
    b.entry(&"cut".into(), V::Object(truncated));
    b.entry(&"depth".into(), V::Truncated(crate::Limit::Depth));
    let fields = b.entry_range(start);
    b.set_object(
        instance,
        O::Instance {
            type_arguments,
            declaration,
            fields,
            original_len: 13,
        },
    );
    let name = b.string(&"user.Extract".into()).unwrap();
    b.set_object(
        function,
        O::Descriptive {
            kind: crate::Description::Function,
            name: Some(name),
        },
    );
    b.set_object(opaque, O::NonSnapshotableValue {});
    b.set_object(truncated, O::Truncated(crate::Limit::Bytes));
    let start = b.value_start();
    b.push_value(V::Object(instance));
    b.push_value(V::OmittedArg);
    b.push_value(V::Object(list));
    let slots = b.value_range(start);
    b.finish_args(3, slots)
}

#[test]
fn every_value_kind_round_trips_with_verified_identity_and_preserved_graph() {
    let pool = SnapshotPool::new(1, Limits::default());
    let snapshot = rich_snapshot(&pool);
    let decoded = decode(&encode(&snapshot)).unwrap();
    assert_eq!(decoded.id, snapshot.id());
    assert!(!decoded.limited);
    let DecodedRoot::FunctionArgs {
        parameter_count,
        slots,
    } = &decoded.root
    else {
        panic!("argument root")
    };
    assert_eq!(*parameter_count, 3);
    assert_eq!(slots[1], DecodedValue::OmittedArg);
    // Shared references keep one object; the list still contains itself.
    let (DecodedValue::Object(instance), DecodedValue::Object(list)) = (&slots[0], &slots[2])
    else {
        panic!("object slots")
    };
    let DecodedObject::List {
        items,
        original_len,
        ..
    } = decoded.object(*list)
    else {
        panic!("list")
    };
    assert_eq!(*original_len, 9);
    assert_eq!(items[3], DecodedValue::Object(*list));
    let DecodedValue::Float(nan) = items[1] else {
        panic!("float")
    };
    assert_eq!(
        nan.to_bits(),
        0x7ff8_0000_0000_1234,
        "NaN payload preserved"
    );
    let DecodedObject::Instance {
        declaration,
        fields,
        ..
    } = decoded.object(*instance)
    else {
        panic!("instance")
    };
    assert!(matches!(
        decoded.object(*declaration),
        DecodedObject::Declaration { tag, .. } if *tag == TypeTag::from_i64(42)
    ));
    let field = |name: &str| &fields.iter().find(|(key, _)| &**key == name).unwrap().1;
    assert_eq!(field("name"), &DecodedValue::String("hello λ".into()));
    assert_eq!(field("shared"), &DecodedValue::Object(*list));
    assert_eq!(
        field("big"),
        &DecodedValue::Bigint(Box::new(BigInt::from(-1) << 100))
    );
    assert!(
        matches!(field("status"), DecodedValue::Enum { variant: 1, name, .. } if &**name == "Active")
    );
    let DecodedValue::Object(cell) = field("cell") else {
        panic!("cell")
    };
    assert_eq!(
        decoded.object(*cell),
        &DecodedObject::Cell(DecodedValue::Object(*cell))
    );
}

#[test]
fn value_roots_and_limited_captures_verify() {
    let pool = SnapshotPool::new(
        1,
        Limits {
            max_bytes: Some(2),
            ..Limits::default()
        },
    );
    let mut b = pool.try_acquire().unwrap();
    let value = b
        .string(&"too long".into())
        .map_or(V::Truncated(crate::Limit::Bytes), V::String);
    let snapshot = b.finish_value(value);
    assert!(snapshot.stats().limited);
    let decoded = decode(&encode(&snapshot)).unwrap();
    assert!(decoded.limited);
    assert_eq!(
        decoded.root,
        DecodedRoot::Value(DecodedValue::Truncated(crate::Limit::Bytes))
    );
    drop(snapshot);
    let empty = pool.try_acquire().unwrap().finish_value(V::Null);
    assert_eq!(decode(&encode(&empty)).unwrap().id, empty.id());
}

#[test]
fn content_changes_are_rejected_even_when_the_header_is_intact() {
    let pool = SnapshotPool::new(1, Limits::default());
    let snapshot = rich_snapshot(&pool);
    let bytes = encode(&snapshot);
    // "hello λ" appears in a string leaf; changing one letter keeps a
    // well-formed blob with the original header ID.
    let at = bytes.windows(5).position(|w| w == b"hello").unwrap();
    let mut changed = bytes.clone();
    changed[at] = b'j';
    assert!(matches!(
        decode(&changed),
        Err(BlobError::IdMismatch { declared, .. }) if declared == snapshot.id()
    ));
    // A different declared ID with unchanged content is also rejected.
    let mut header = bytes;
    header[12] ^= 1;
    assert!(matches!(decode(&header), Err(BlobError::IdMismatch { .. })));
}

#[test]
fn malformed_blobs_fail_without_panicking() {
    let pool = SnapshotPool::new(1, Limits::default());
    let bytes = encode(&rich_snapshot(&pool));
    for len in 0..bytes.len() {
        assert!(decode(&bytes[..len]).is_err(), "prefix {len} accepted");
    }
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(matches!(decode(&extra), Err(BlobError::Invalid(_))));
    let mut magic = bytes.clone();
    magic[0] = b'X';
    assert_eq!(decode(&magic), Err(BlobError::Magic));
    // The previous format (attribute-carrying types, v1 hash domain) and any
    // later one are rejected by version, before their content is read.
    for other in [1, crate::BLOB_VERSION + 1] {
        let mut version = bytes.clone();
        version[8..12].copy_from_slice(&other.to_le_bytes());
        assert_eq!(decode(&version), Err(BlobError::Version(other)));
    }
    // Every single-byte corruption is either rejected or (for bytes the hash
    // does not cover, e.g. none) detected; it must never panic.
    for index in 0..bytes.len() {
        let mut corrupt = bytes.clone();
        corrupt[index] = corrupt[index].wrapping_add(0x55);
        assert!(decode(&corrupt).is_err(), "corruption at {index} accepted");
    }
}

#[test]
fn out_of_range_references_are_structural_errors() {
    // Header for an empty graph whose value root references object 0.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&crate::BLOB_MAGIC);
    bytes.extend_from_slice(&crate::BLOB_VERSION.to_le_bytes());
    bytes.extend_from_slice(&[0; 16]);
    bytes.push(0); // not limited
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.push(0); // value root
    bytes.push(7); // object reference
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    assert!(
        matches!(decode(&bytes), Err(BlobError::Invalid(reason)) if reason.contains("out of range"))
    );
}

#[test]
fn limits_are_enforced_before_allocation() {
    let pool = SnapshotPool::new(1, Limits::default());
    let bytes = encode(&rich_snapshot(&pool));
    let tight = |limits: DecodeLimits| decode_blob(&bytes, &limits);
    assert_eq!(
        tight(DecodeLimits {
            max_objects: 3,
            ..DecodeLimits::default()
        }),
        Err(BlobError::Limit("objects"))
    );
    assert_eq!(
        tight(DecodeLimits {
            max_values: 4,
            ..DecodeLimits::default()
        }),
        Err(BlobError::Limit("values"))
    );
    assert_eq!(
        tight(DecodeLimits {
            max_decoded_bytes: 64,
            ..DecodeLimits::default()
        }),
        Err(BlobError::Limit("decoded bytes"))
    );
    // A declared length far beyond the input is rejected before allocating.
    let mut huge = Vec::new();
    huge.extend_from_slice(&crate::BLOB_MAGIC);
    huge.extend_from_slice(&crate::BLOB_VERSION.to_le_bytes());
    huge.extend_from_slice(&[0; 16]);
    huge.push(0);
    huge.extend_from_slice(&0_u32.to_le_bytes());
    huge.push(1); // argument root
    huge.extend_from_slice(&3_u64.to_le_bytes());
    huge.extend_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(decode(&huge), Err(BlobError::Truncated));
}

fn nested_list(depth: usize) -> OwnedType {
    let mut ty = int_type();
    for _ in 0..depth {
        ty = RealizedTy::List(Box::new(ty));
    }
    ty
}

fn on_large_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

fn type_blob(ty: OwnedType) -> Vec<u8> {
    let pool = SnapshotPool::new(1, Limits::default());
    let mut b = pool.try_acquire().unwrap();
    let id = b.push_type(ty);
    encode(&b.finish_value(V::Type(id)))
}

#[test]
fn type_descriptions_are_bounded_and_large_ones_never_recurse_on_the_caller() {
    let limits = DecodeLimits::default();
    // Serializing and dropping deep types recurses too: build them elsewhere.
    // Attribute-free types spend one byte per list level, so the in-place
    // bound admits the most nesting it ever has: check that worst case too.
    let int_bytes = borsh::to_vec(&int_type()).unwrap().len();
    let per_level = borsh::to_vec(&nested_list(1)).unwrap().len() - int_bytes;
    let deepest_shallow = (SHALLOW_TYPE_BYTES - int_bytes) / per_level;
    let (shallow, deepest_in_place, deep, too_deep) = on_large_stack(move || {
        let deepest = (limits.max_type_bytes - 16) / per_level;
        (
            type_blob(nested_list(3)),
            type_blob(nested_list(deepest_shallow)),
            type_blob(nested_list(deepest)),
            type_blob(nested_list(limits.max_type_bytes)),
        )
    });
    // Decode on a thread with the default 2 MiB stack, in debug builds too.
    let (shallow, deepest_in_place, deep, too_deep) = std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || {
            let decoded = (
                decode(&shallow),
                decode(&deepest_in_place),
                decode(&deep),
                decode(&too_deep),
            );
            // The in-place type is deep too: measure it and drop it here.
            let in_place = decoded
                .1
                .as_ref()
                .map(|decoded| match &decoded.root {
                    DecodedRoot::Value(DecodedValue::Type(ty)) => {
                        (ty.encoded.len(), ty.decoded.is_some())
                    }
                    other => panic!("type root: {other:?}"),
                })
                .map_err(Clone::clone);
            (decoded.0, in_place, decoded.2, decoded.3)
        })
        .unwrap()
        .join()
        .expect("type decoding must not exhaust a 2 MiB stack");
    let description = |decoded: DecodedSnapshot| match decoded.root {
        DecodedRoot::Value(DecodedValue::Type(ty)) => ty,
        other => panic!("type root: {other:?}"),
    };
    let shallow = description(shallow.unwrap());
    assert!(shallow.encoded.len() <= SHALLOW_TYPE_BYTES);
    assert_eq!(shallow.decoded.as_deref(), Some(&nested_list(3)));
    let (in_place_bytes, in_place_decoded) = deepest_in_place.unwrap();
    assert!(
        in_place_bytes <= SHALLOW_TYPE_BYTES && in_place_bytes > SHALLOW_TYPE_BYTES - per_level
    );
    assert!(
        in_place_decoded,
        "the deepest shallow description decodes in place"
    );
    let deep = description(deep.unwrap());
    assert!(deep.encoded.len() > SHALLOW_TYPE_BYTES);
    assert!(deep.decoded.is_none(), "deep descriptions stay encoded");
    assert_eq!(too_deep, Err(BlobError::Limit("type description bytes")));
}

#[test]
fn map_entry_keys_hash_like_the_producer() {
    // Keys and values from separate entry ranges must verify independently.
    let pool = SnapshotPool::new(1, Limits::default());
    let mut b = pool.try_acquire().unwrap();
    let map = b.reserve_object().unwrap();
    let key = b.push_type(int_type());
    let value = b.push_type(int_type());
    let start = b.entry_start();
    for (k, v) in [("b", 2), ("a", 1)] {
        b.entry(&k.into(), V::Int(v));
    }
    let entries = b.entry_range(start);
    b.set_object(
        map,
        O::Map {
            key_type: key,
            value_type: value,
            entries,
            original_len: 2,
        },
    );
    let snapshot = b.finish_value(V::Object(map));
    let decoded = decode(&encode(&snapshot)).unwrap();
    let DecodedObject::Map { entries, .. } = decoded.object(0) else {
        panic!("map")
    };
    assert_eq!(
        entries,
        &vec![
            ("b".into(), DecodedValue::Int(2)),
            ("a".into(), DecodedValue::Int(1))
        ]
    );
    let _: &[MapEntry] = snapshot.entries(entries_range(&snapshot));
}

fn entries_range(snapshot: &Snapshot) -> crate::Range<MapEntry> {
    match snapshot.object(crate::ObjectId(0)) {
        O::Map { entries, .. } => *entries,
        _ => unreachable!(),
    }
}

#[test]
#[ignore = "measurement"]
#[allow(clippy::print_stderr, reason = "prints the measured stack cost")]
fn measure_type_decode_stack_per_level() {
    // A stack overflow aborts the process, so each probe runs in a child.
    if let (Ok(depth), Ok(stack)) = (std::env::var("PROBE_DEPTH"), std::env::var("PROBE_STACK")) {
        let bytes = borsh::to_vec(&nested_list(depth.parse().unwrap())).unwrap();
        std::thread::Builder::new()
            .stack_size(stack.parse::<usize>().unwrap() << 10)
            .spawn(move || std::mem::forget(OwnedType::deserialize_reader(&mut bytes.as_slice())))
            .unwrap()
            .join()
            .unwrap();
        return;
    }
    for depth in [64usize, 128, 256, 512, 1024, 2048] {
        for stack_kib in [32usize, 64, 128, 256, 512, 1024, 2048, 4096] {
            let ok = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "decode::tests::measure_type_decode_stack_per_level",
                    "--ignored",
                ])
                .env("PROBE_DEPTH", depth.to_string())
                .env("PROBE_STACK", stack_kib.to_string())
                .output()
                .unwrap()
                .status
                .success();
            if ok {
                eprintln!("depth {depth}: fits in {stack_kib} KiB");
                break;
            }
        }
    }
}
