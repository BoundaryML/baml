//! Context uses ordinary map snapshots and the existing CAS hash/codec.

use baml_type::RealizedTy;
use btel_types::context::{Context, ContextValue};

use crate::{Builder, Shaper, Snapshot, SnapshotObject, SnapshotPool, SnapshotValue};

/// Capture an entire context or decline it. A truncated metadata map must not
/// look like a complete context with missing keys. Pool admission never affects
/// the live execution context.
pub fn capture(context: &Context, pool: &SnapshotPool) -> Option<Snapshot> {
    capture_with_builder(context, pool.try_acquire()?, &mut Shaper::default())
}

/// As [`capture`], into `builder`, shaped with the capturing thread's
/// `shaper`.
pub fn capture_with_builder(
    context: &Context,
    mut builder: Builder,
    shaper: &mut Shaper,
) -> Option<Snapshot> {
    if builder.limits().max_depth.is_some_and(|depth| depth < 2) {
        return None;
    }
    let root = builder.reserve_object()?;
    let metadata = builder.reserve_object()?;
    let key_type = builder.push_type(RealizedTy::String);
    let value_type = builder.push_type(RealizedTy::Unknown);
    let start = builder.entry_start();
    // BTreeMap ordering makes the hash independent of patch construction order.
    for (key, value) in context.metadata() {
        let value = match value {
            ContextValue::String(text) => {
                SnapshotValue::String(builder.string(&text.as_str().into())?)
            }
            ContextValue::Int(value) => SnapshotValue::Int(*value),
            ContextValue::Float(value) => SnapshotValue::Float(*value),
            ContextValue::Bool(value) => SnapshotValue::Bool(*value),
        };
        entry(&mut builder, key, value)?;
    }
    let entries = builder.entry_range(start);
    builder.set_object(
        metadata,
        SnapshotObject::Map {
            key_type,
            value_type,
            entries,
            original_len: context.metadata().len(),
        },
    );
    let identity = match context.distinct_id() {
        Some(value) => SnapshotValue::String(builder.string(&value.into())?),
        None => SnapshotValue::Null,
    };
    let start = builder.entry_start();
    entry(&mut builder, "distinct_id", identity)?;
    entry(&mut builder, "metadata", SnapshotValue::Object(metadata))?;
    let entries = builder.entry_range(start);
    builder.set_object(
        root,
        SnapshotObject::Map {
            key_type,
            value_type,
            entries,
            original_len: 2,
        },
    );
    Some(builder.finish_value(SnapshotValue::Object(root), shaper))
}

fn entry(builder: &mut Builder, key: &str, value: SnapshotValue) -> Option<()> {
    if builder.remaining_entries() == 0 {
        return None;
    }
    builder.entry(&key.into(), value);
    Some(())
}

#[cfg(test)]
mod tests {
    use btel_types::context::ContextPatch;

    use super::*;
    use crate::{BlobScratch, DecodeLimits, Limits, SnapshotRoot, decode_blob};

    fn context() -> Context {
        Context::default().with_patch(&ContextPatch {
            metadata: [
                (
                    "text".into(),
                    Some(ContextValue::String("quote=\"\n\t\\".into())),
                ),
                ("int".into(), Some(ContextValue::Int(-42))),
                ("float".into(), Some(ContextValue::Float(-0.0))),
                ("bool".into(), Some(ContextValue::Bool(false))),
            ]
            .into(),
            distinct_id: Some("user-42".into()),
        })
    }

    #[test]
    fn context_uses_the_existing_snapshot_codec() {
        let pool = SnapshotPool::new(2, Limits::default());
        let snapshot = capture(&context(), &pool).unwrap();
        let mut bytes = Vec::new();
        snapshot
            .root_blob()
            .write(&mut BlobScratch::default(), &mut bytes)
            .unwrap();
        let decoded = decode_blob(&bytes, &DecodeLimits::default()).unwrap();
        assert_eq!(decoded.id, snapshot.root_id());
        assert!(!snapshot.stats().limited);
        let SnapshotRoot::Value(SnapshotValue::Object(root)) = snapshot.root() else {
            panic!("context root must be a map");
        };
        let SnapshotObject::Map { entries, .. } = snapshot.object(root) else {
            panic!("context root must be a map");
        };
        let entries = snapshot.entries(*entries);
        assert_eq!(entries[0].key.as_str(), "distinct_id");
        let SnapshotValue::String(identity) = entries[0].value else {
            panic!("identity must be a string");
        };
        assert_eq!(snapshot.string(identity).as_str(), "user-42");
        let SnapshotValue::Object(metadata) = entries[1].value else {
            panic!("metadata must be a map");
        };
        let SnapshotObject::Map { entries, .. } = snapshot.object(metadata) else {
            panic!("metadata must be a map");
        };
        let entries = snapshot.entries(*entries);
        assert_eq!(entries[0].value, SnapshotValue::Bool(false));
        let SnapshotValue::Float(float) = entries[1].value else {
            panic!("float must retain its type");
        };
        assert_eq!(float.to_bits(), (-0.0_f64).to_bits());
        assert_eq!(entries[2].value, SnapshotValue::Int(-42));
        let SnapshotValue::String(text) = entries[3].value else {
            panic!("text must retain its type");
        };
        assert_eq!(snapshot.string(text).as_str(), "quote=\"\n\t\\");
    }

    #[test]
    fn equal_contexts_have_equal_ids_regardless_of_patch_history() {
        let pool = SnapshotPool::new(2, Limits::default());
        let context = context();
        let mut rebuilt = Context::default();
        for (key, value) in context.metadata().iter().rev() {
            rebuilt = rebuilt.with_patch(&ContextPatch {
                metadata: [(key.clone(), Some(value.clone()))].into(),
                distinct_id: context.distinct_id().map(str::to_owned),
            });
        }
        let first = capture(&context, &pool).unwrap();
        let second = capture(&rebuilt, &pool).unwrap();
        assert_eq!(first.root_id(), second.root_id());
    }

    #[test]
    fn admission_failure_does_not_change_context() {
        let context = context();
        let pool = SnapshotPool::new(1, Limits::default());
        let held = capture(&context, &pool).unwrap();
        assert!(capture(&context, &pool).is_none());
        assert_eq!(context.distinct_id(), Some("user-42"));
        assert_eq!(context.metadata().len(), 4);
        drop(held);
        assert!(capture(&context, &pool).is_some());
    }

    #[test]
    fn limits_decline_incomplete_contexts_and_recycle_storage() {
        for limits in [
            Limits {
                max_values: Some(1),
                ..Limits::default()
            },
            Limits {
                max_objects: Some(1),
                ..Limits::default()
            },
            Limits {
                max_depth: Some(1),
                ..Limits::default()
            },
        ] {
            let pool = SnapshotPool::new(1, limits);
            assert!(capture(&context(), &pool).is_none());
            assert!(pool.try_acquire().is_some());
        }
    }
}
