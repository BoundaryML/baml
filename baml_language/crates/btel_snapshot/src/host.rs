//! Bounded, owned host observations supplied by adapters without invoking user
//! serializers, getters, iterators or display hooks. No live host references.
use crate::{Builder, Limit, Snapshot, SnapshotObject, SnapshotValue};

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
    builder.finish_value(value)
}

fn add(
    builder: &mut Builder,
    value: &HostValue,
    depth: usize,
    remaining: &mut usize,
    bytes: &mut usize,
) -> SnapshotValue {
    if depth > MAX_DEPTH {
        return builder.limited(Limit::Depth);
    }
    if *remaining == 0 {
        return builder.limited(Limit::Values);
    }
    *remaining -= 1;
    match value {
        HostValue::Null => SnapshotValue::Null,
        HostValue::Bool(v) => SnapshotValue::Bool(*v),
        HostValue::Int(v) => SnapshotValue::Int(*v),
        HostValue::Float(v) => SnapshotValue::Float(*v),
        HostValue::String(v) => {
            if v.len() > *bytes {
                return builder.limited(Limit::Bytes);
            }
            *bytes -= v.len();
            match builder.string(&v.as_str().into()) {
                Some(value) => SnapshotValue::String(value),
                None => builder.limited(Limit::Bytes),
            }
        }
        HostValue::Truncated(reason) => builder.limited(*reason),
        HostValue::Unavailable => {
            let Some(id) = builder.reserve_object() else {
                return builder.limited(Limit::Objects);
            };
            builder.set_object(id, SnapshotObject::NonSnapshotableValue {});
            SnapshotValue::Object(id)
        }
        HostValue::List(values) => {
            if values.len() > *remaining {
                return builder.limited(Limit::Values);
            }
            let Some(id) = builder.reserve_object() else {
                return builder.limited(Limit::Objects);
            };
            let mut copied = Vec::new();
            for value in values {
                if *remaining == 0 {
                    return builder.limited(Limit::Values);
                }
                copied.push(add(builder, value, depth + 1, remaining, bytes));
            }
            let start = builder.value_start();
            for value in copied {
                if builder.remaining_values() == 0 {
                    builder.limited(Limit::Values);
                    break;
                }
                builder.push_value(value);
            }
            let items = builder.value_range(start);
            let element_type = builder.push_type(baml_type::RealizedTy::Unknown);
            builder.set_object(
                id,
                SnapshotObject::List {
                    element_type,
                    items,
                    original_len: match value {
                        HostValue::List(values) => values.len(),
                        _ => unreachable!(),
                    },
                },
            );
            SnapshotValue::Object(id)
        }
        HostValue::Map(entries) => {
            if entries.len() > *remaining {
                return builder.limited(Limit::Values);
            }
            let Some(key_bytes) = entries
                .iter()
                .try_fold(0usize, |size, (key, _)| size.checked_add(key.len()))
            else {
                return builder.limited(Limit::Bytes);
            };
            if key_bytes > *bytes {
                return builder.limited(Limit::Bytes);
            }
            *bytes -= key_bytes;
            let Some(id) = builder.reserve_object() else {
                return builder.limited(Limit::Objects);
            };
            let mut values = Vec::new();
            for (key, value) in entries {
                if *remaining == 0 {
                    return builder.limited(Limit::Values);
                }
                values.push((key, add(builder, value, depth + 1, remaining, bytes)));
            }
            let start = builder.entry_start();
            for (key, value) in values {
                if builder.remaining_entries() == 0 {
                    builder.limited(Limit::Values);
                    break;
                }
                if !builder.content(key.len(), false) {
                    break;
                }
                builder.entry(&key.as_str().into(), value);
            }
            let fields = builder.entry_range(start);
            let key_type = builder.push_type(baml_type::RealizedTy::String);
            let value_type = builder.push_type(baml_type::RealizedTy::Unknown);
            builder.set_object(
                id,
                SnapshotObject::Map {
                    key_type,
                    value_type,
                    entries: fields,
                    original_len: entries.len(),
                },
            );
            SnapshotValue::Object(id)
        }
    }
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
        oversized.write_blob(&mut blob).unwrap();
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
        snapshot.write_blob(&mut blob).unwrap();
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
        snapshot.write_blob(&mut blob).unwrap();
        crate::decode_blob(&blob, &DecodeLimits::default()).unwrap();
    }
}
