//! Bounded, owned host observations supplied by adapters without invoking user
//! serializers, getters, iterators or display hooks. No live host references.
use crate::{Builder, Limit, OwnedType, Shaper, Snapshot, SnapshotObject, SnapshotValue};

#[derive(Debug)]
pub enum HostValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<Self>),
    Map(Vec<(String, Self)>),
    Unavailable,
    Truncated(Limit),
}

/// Adapter traversal must obey these bounds before allocating this tree. The
/// snapshot builder independently enforces its existing transport budgets.
pub const MAX_DEPTH: usize = 8;
pub const MAX_VALUES: usize = 512;
pub const MAX_BYTES: usize = 64 * 1024;

pub fn capture(mut builder: Builder, value: &HostValue) -> Snapshot {
    let mut remaining = MAX_VALUES;
    let mut bytes = MAX_BYTES;
    let value = add(&mut builder, value, 0, &mut remaining, &mut bytes);
    // Host observations are small and arrive from adapter threads, which
    // hold no shaping scratch of their own.
    builder.finish(value, &mut Shaper::default())
}

fn add(
    builder: &mut Builder,
    value: &HostValue,
    depth: usize,
    remaining: &mut usize,
    bytes: &mut usize,
) -> SnapshotValue {
    if depth > MAX_DEPTH {
        return SnapshotValue::Truncated(Limit::Depth);
    }
    if *remaining == 0 {
        return SnapshotValue::Truncated(Limit::Values);
    }
    *remaining -= 1;
    match value {
        HostValue::Null => SnapshotValue::Null,
        HostValue::Bool(v) => SnapshotValue::Bool(*v),
        HostValue::Int(v) => SnapshotValue::Int(*v),
        HostValue::Float(v) => SnapshotValue::Float(*v),
        HostValue::String(v) => {
            if v.len() > *bytes {
                return SnapshotValue::Truncated(Limit::Bytes);
            }
            *bytes -= v.len();
            builder.leaves().string_value(&v.as_str().into())
        }
        HostValue::Truncated(reason) => SnapshotValue::Truncated(*reason),
        HostValue::Unavailable => object(builder, SnapshotObject::NonSnapshotableValue {}),
        HostValue::List(values) => {
            if values.len() > *remaining {
                return SnapshotValue::Truncated(Limit::Values);
            }
            // A container's values are one run, so what it holds is built
            // before it.
            let mut copied = Vec::with_capacity(values.len());
            for value in values {
                if *remaining == 0 {
                    return SnapshotValue::Truncated(Limit::Values);
                }
                copied.push(add(builder, value, depth + 1, remaining, bytes));
            }
            let element_type = builder.leaves().ty(OwnedType::Unknown);
            let list = builder.list(element_type, copied.into_iter(), |_, value| value);
            object(builder, list)
        }
        HostValue::Map(entries) => {
            if entries.len() > *remaining {
                return SnapshotValue::Truncated(Limit::Values);
            }
            let Some(key_bytes) = entries
                .iter()
                .try_fold(0usize, |size, (key, _)| size.checked_add(key.len()))
            else {
                return SnapshotValue::Truncated(Limit::Bytes);
            };
            if key_bytes > *bytes {
                return SnapshotValue::Truncated(Limit::Bytes);
            }
            *bytes -= key_bytes;
            let mut values = Vec::with_capacity(entries.len());
            for (key, value) in entries {
                if *remaining == 0 {
                    return SnapshotValue::Truncated(Limit::Values);
                }
                values.push((key, add(builder, value, depth + 1, remaining, bytes)));
            }
            let key_type = builder.leaves().ty(OwnedType::string());
            let value_type = builder.leaves().ty(OwnedType::Unknown);
            let map = builder.map(
                key_type,
                value_type,
                values.into_iter(),
                |_, (key, value)| (key.as_str().into(), value),
            );
            object(builder, map)
        }
    }
}

fn object(builder: &mut Builder, object: SnapshotObject) -> SnapshotValue {
    builder.leaves().object(object).map_or(
        SnapshotValue::Truncated(Limit::Objects),
        SnapshotValue::Object,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DecodeLimits, Limits, SnapshotPool, SnapshotRoot};

    #[test]
    fn unsupported_and_oversized_host_values_are_explicit_observations() {
        let pool = SnapshotPool::new(1, Limits::default());
        let unavailable = capture(pool.try_acquire().unwrap(), &HostValue::Unavailable);
        let SnapshotRoot::Value(SnapshotValue::Object(id)) = unavailable.root() else {
            panic!("unavailable value must be represented");
        };
        assert!(matches!(
            unavailable.object(id),
            SnapshotObject::NonSnapshotableValue {}
        ));
        drop(unavailable);
        let oversized = capture(
            pool.try_acquire().unwrap(),
            &HostValue::String("x".repeat(MAX_BYTES + 1)),
        );
        assert!(oversized.stats().limited);
        assert!(matches!(
            oversized.root(),
            SnapshotRoot::Value(SnapshotValue::Truncated(Limit::Bytes))
        ));
        let mut blob = vec![];
        oversized
            .root_blob()
            .write(&mut crate::BlobScratch::default(), &mut blob)
            .unwrap();
        crate::decode_blob(&blob, &DecodeLimits::default()).unwrap();
    }

    #[test]
    fn nested_host_maps_and_lists_produce_self_contained_blobs() {
        let pool = SnapshotPool::new(1, Limits::default());
        let value = HostValue::Map(vec![(
            "arguments".into(),
            HostValue::List(vec![
                HostValue::Map(vec![("value".into(), HostValue::Int(7))]),
                HostValue::String("output".into()),
            ]),
        )]);
        let snapshot = capture(pool.try_acquire().unwrap(), &value);
        let mut blob = vec![];
        snapshot
            .root_blob()
            .write(&mut crate::BlobScratch::default(), &mut blob)
            .unwrap();
        crate::decode_blob(&blob, &DecodeLimits::default()).unwrap();
        assert!(!snapshot.stats().limited);
    }

    #[test]
    fn transport_limits_are_visible_in_host_snapshots() {
        let pool = SnapshotPool::new(
            1,
            Limits {
                max_values: Some(0),
                ..Limits::default()
            },
        );
        let snapshot = capture(
            pool.try_acquire().unwrap(),
            &HostValue::List(vec![HostValue::Int(7)]),
        );
        assert!(snapshot.stats().limited);
        let mut blob = vec![];
        snapshot
            .root_blob()
            .write(&mut crate::BlobScratch::default(), &mut blob)
            .unwrap();
        crate::decode_blob(&blob, &DecodeLimits::default()).unwrap();
    }
}
