//! Customer content size, independent of encoding and CAS storage splitting.
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    Blob, MediaSource, SnapshotObject, SnapshotRoot, SnapshotValue, shape::LogicalBytesV1,
};

impl Blob<'_> {
    /// Total logical content of this value, including values stored in children.
    ///
    /// Strings, keys, field names and enum variants count UTF-8 bytes; booleans
    /// count one byte; integers and floats eight; bigints their magnitude bytes;
    /// byte arrays their captured bytes. Container framing, type/declaration
    /// descriptions, omitted values and uncaptured/truncated content count zero.
    /// Every reference counts its content again. A recursive cycle backedge
    /// counts only its 16-byte CAS ID. Media content counts decoded bytes,
    /// not base64 framing; invalid base64 counts the captured text instead.
    /// URL/path and MIME text count too. None means the expanded size exceeds
    /// u64. Traversal is iterative and acyclic object totals are memoized.
    pub fn logical_bytes_v1(&self) -> Option<u64> {
        enum Task {
            Value(SnapshotValue),
            Finish(crate::ObjectId, u64, u64),
        }
        fn media_bytes(text: &str) -> u64 {
            // Stream decoding into a sink: no allocation of a second payload.
            let mut decoded = base64::read::DecoderReader::new(
                text.as_bytes(),
                &base64::engine::general_purpose::STANDARD,
            );
            std::io::copy(&mut decoded, &mut std::io::sink()).unwrap_or(text.len() as u64)
        }
        if let LogicalBytesV1::Measured(size) = self.entry().logical_bytes_v1 {
            return size;
        }
        let snapshot = self.snapshot;
        let mut pending = match self.entry().root {
            SnapshotRoot::Value(value) => vec![Task::Value(value)],
            SnapshotRoot::FunctionArgs(args) => snapshot
                .values(args.slots)
                .iter()
                .copied()
                .map(Task::Value)
                .collect(),
        };
        let mut active = FxHashSet::default();
        let mut completed = FxHashMap::default();
        let mut cycles = 0;
        let mut total = 0u64;
        while let Some(task) = pending.pop() {
            let value = match task {
                Task::Value(value) => value,
                Task::Finish(id, before, cycle_count) => {
                    active.remove(&id);
                    if cycles == cycle_count {
                        completed.insert(id, total - before);
                    }
                    continue;
                }
            };
            let bytes = match value {
                SnapshotValue::Null
                | SnapshotValue::OmittedArg
                | SnapshotValue::Truncated(_)
                | SnapshotValue::Type(_) => 0,
                SnapshotValue::Bool(_) => 1,
                SnapshotValue::Int(_) | SnapshotValue::Float(_) => 8,
                SnapshotValue::String(id) => snapshot.string(id).len() as u64,
                SnapshotValue::Bigint(id) => snapshot.bigint(id).bits().div_ceil(8),
                SnapshotValue::Enum { name, .. } => snapshot.label(name).len() as u64,
                SnapshotValue::Object(id) if active.contains(&id) => {
                    cycles += 1;
                    std::mem::size_of::<crate::CasId>() as u64
                }
                SnapshotValue::Object(id) if completed.contains_key(&id) => completed[&id],
                SnapshotValue::Object(id) => {
                    // Child blobs were measured first. Their root total is
                    // reusable; a non-root member of a cycle has its own walk.
                    if let Some(home) = snapshot.0.shape.object_home(id)
                        && home.blob != self.index()
                        && home.node == 0
                        && !active.iter().any(|active_id| {
                            snapshot
                                .0
                                .shape
                                .object_home(*active_id)
                                .is_some_and(|active_home| active_home.blob == home.blob)
                        })
                        && let LogicalBytesV1::Measured(size) =
                            snapshot.0.shape.blobs[home.blob.0 as usize].logical_bytes_v1
                    {
                        total = total.checked_add(size?)?;
                        continue;
                    }
                    active.insert(id);
                    pending.push(Task::Finish(id, total, cycles));
                    match snapshot.object(id) {
                        SnapshotObject::Uint8Array { data } => data.len() as u64,
                        SnapshotObject::List { items, .. } => {
                            pending
                                .extend(snapshot.values(*items).iter().copied().map(Task::Value));
                            0
                        }
                        SnapshotObject::Map { entries, .. } => {
                            for entry in snapshot.entries(*entries) {
                                pending.extend([entry.key, entry.value].map(Task::Value));
                            }
                            0
                        }
                        SnapshotObject::Instance { fields, .. } => {
                            let mut keys = 0;
                            for field in snapshot.fields(*fields) {
                                keys = u64::checked_add(keys, field.key.len() as u64)?;
                                pending.push(Task::Value(field.value));
                            }
                            keys
                        }
                        SnapshotObject::Cell(value) => {
                            pending.push(Task::Value(*value));
                            0
                        }
                        SnapshotObject::Media {
                            mime_type, source, ..
                        } => {
                            let metadata =
                                mime_type.map_or(0, |id| snapshot.label(id).len() as u64);
                            metadata
                                + match *source {
                                    MediaSource::Url { url, data } => {
                                        snapshot.label(url).len() as u64
                                            + data.map_or(0, |id| {
                                                media_bytes(snapshot.string(id).as_str())
                                            })
                                    }
                                    MediaSource::File { path, data } => {
                                        snapshot.label(path).len() as u64
                                            + data.map_or(0, |id| {
                                                media_bytes(snapshot.string(id).as_str())
                                            })
                                    }
                                    MediaSource::Base64 { data } => {
                                        media_bytes(snapshot.string(data).as_str())
                                    }
                                }
                        }
                        SnapshotObject::Declaration { .. }
                        | SnapshotObject::Uint8ArrayTruncated { .. }
                        | SnapshotObject::NonSnapshotableValue { .. }
                        | SnapshotObject::Descriptive { .. }
                        | SnapshotObject::Truncated(_) => 0,
                    }
                }
            };
            total = total.checked_add(bytes)?;
        }
        Some(total)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use num_bigint::BigInt;

    use super::*;
    use crate::{Builder, Limit, Limits, OwnedType, ShapePolicy, Shaper, Snapshot, SnapshotPool};

    fn object(builder: &mut Builder, value: SnapshotObject) -> SnapshotValue {
        SnapshotValue::Object(builder.leaves().object(value).unwrap())
    }

    #[test]
    fn exact_content_bytes_exclude_types_and_uncaptured_values() {
        let pool = SnapshotPool::new(1, Limits::default());
        let mut b = pool.try_acquire().unwrap();
        let text = b.leaves().string_value(&"é".into());
        let bigint = b.leaves().bigint(&Arc::new(BigInt::from(256)));
        let ty = b.leaves().ty(OwnedType::string());
        let bytes = b.bytes(&[0; 3]);
        let bytes = object(&mut b, bytes);
        let unavailable = object(
            &mut b,
            SnapshotObject::Uint8ArrayTruncated { original_len: 999 },
        );
        let root = b.arguments(
            [
                SnapshotValue::Null,
                SnapshotValue::OmittedArg,
                SnapshotValue::Bool(true),
                SnapshotValue::Int(7),
                SnapshotValue::Float(1.5),
                text,
                bigint,
                bytes,
                SnapshotValue::Type(ty),
                SnapshotValue::Truncated(Limit::Bytes),
                unavailable,
            ]
            .into_iter(),
            |_, value| value,
        );
        let snapshot = b.finish(root, &mut Shaper::default());
        assert_eq!(
            snapshot.root_blob().logical_bytes_v1(),
            Some(1 + 8 + 8 + 2 + 2 + 3)
        );
    }

    fn aliases(pool: &SnapshotPool, policy: ShapePolicy) -> Snapshot {
        let mut b = pool.try_acquire().unwrap();
        let ty = b.leaves().ty(OwnedType::unknown());
        let child = b.list(ty, ["é", "abc"].into_iter(), |leaves, text| {
            leaves.string_value(&text.into())
        });
        let child = object(&mut b, child);
        let root = b.map(
            ty,
            ty,
            [child, child].into_iter().enumerate(),
            |leaves, (i, child)| (leaves.string_value(&format!("k{i}").into()), child),
        );
        let root = object(&mut b, root);
        b.finish(root, &mut Shaper::new(policy))
    }

    #[test]
    fn aliases_count_each_occurrence_and_split_preserves_totals_after_releasing_leaves() {
        let pool = SnapshotPool::new(1, Limits::default());
        for policy in [
            ShapePolicy::Whole,
            ShapePolicy::Split {
                unit_bytes: 1,
                leaf_bytes: 1,
            },
        ] {
            let snapshot = aliases(&pool, policy);
            assert_eq!(snapshot.root_blob().logical_bytes_v1(), Some(2 + 5 + 2 + 5));
            let root_id = snapshot.root_id();
            let split = snapshot.split();
            // Delivery can drop strings before it reads the parent metadata.
            drop(split.leaves);
            let structure = split.structure.unwrap();
            let root = structure.blobs().find(|blob| blob.id() == root_id).unwrap();
            assert_eq!(root.logical_bytes_v1(), Some(14));
        }
    }

    #[test]
    fn recursive_cycles_count_cas_reference_bytes_but_regular_aliases_repeat_content() {
        let pool = SnapshotPool::new(1, Limits::default());
        let mut b = pool.try_acquire().unwrap();
        let ty = b.leaves().ty(OwnedType::unknown());
        let cycle = b.leaves().reserve().unwrap();
        let id = cycle.id();
        let list = b.list(
            ty,
            [SnapshotValue::Int(7), SnapshotValue::Object(id)].into_iter(),
            |_, v| v,
        );
        b.leaves().fill(cycle, list);
        let root = b.arguments([SnapshotValue::Object(id); 2].into_iter(), |_, v| v);
        let snapshot = b.finish(root, &mut Shaper::default());
        assert_eq!(std::mem::size_of::<crate::CasId>(), 16);
        assert_eq!(snapshot.root_blob().logical_bytes_v1(), Some((8 + 16) * 2));
    }

    #[test]
    fn entering_a_split_cycle_at_either_node_preserves_exact_bytes() {
        let pool = SnapshotPool::new(1, Limits::default());
        for policy in [
            ShapePolicy::Whole,
            ShapePolicy::Split {
                unit_bytes: 1,
                leaf_bytes: 1,
            },
        ] {
            let mut b = pool.try_acquire().unwrap();
            let ty = b.leaves().ty(OwnedType::unknown());
            let a = b.leaves().reserve().unwrap();
            let a_id = a.id();
            let c = b.leaves().reserve().unwrap();
            let c_id = c.id();
            let a_value = b.list(
                ty,
                [SnapshotValue::Int(7), SnapshotValue::Object(c_id)].into_iter(),
                |_, v| v,
            );
            b.leaves().fill(a, a_value);
            let c_value = b.list(
                ty,
                [SnapshotValue::Bool(true), SnapshotValue::Object(a_id)].into_iter(),
                |_, v| v,
            );
            b.leaves().fill(c, c_value);
            let root = b.arguments(
                [SnapshotValue::Object(a_id), SnapshotValue::Object(c_id)].into_iter(),
                |_, v| v,
            );
            let snapshot = b.finish(root, &mut Shaper::new(policy));
            assert_eq!(
                snapshot.root_blob().logical_bytes_v1(),
                Some((8 + 1 + 16) * 2)
            );
            let split = snapshot.split();
            assert_eq!(
                split
                    .structure
                    .unwrap()
                    .blobs()
                    .last()
                    .unwrap()
                    .logical_bytes_v1(),
                Some(50)
            );
        }
    }

    #[test]
    fn media_counts_decoded_content_and_customer_metadata() {
        let pool = SnapshotPool::new(1, Limits::default());
        let mut b = pool.try_acquire().unwrap();
        let data = b.leaves().string(&"YWJjZA==".into()).unwrap();
        let mime_type = b.leaves().label(&"image/png".into());
        let media = object(
            &mut b,
            SnapshotObject::Media {
                kind: baml_type::MediaKind::Image,
                mime_type,
                source: MediaSource::Base64 { data },
            },
        );
        let snapshot = b.finish(
            media,
            &mut Shaper::new(ShapePolicy::Split {
                unit_bytes: 1,
                leaf_bytes: 1,
            }),
        );
        assert_eq!(snapshot.root_blob().logical_bytes_v1(), Some(9 + 4));
        let split = snapshot.split();
        drop(split.leaves);
        assert_eq!(
            split
                .structure
                .unwrap()
                .blobs()
                .last()
                .unwrap()
                .logical_bytes_v1(),
            Some(13)
        );
    }

    #[test]
    fn repeated_dag_is_memoized_and_unrepresentable_total_is_unknown() {
        let pool = SnapshotPool::new(1, Limits::default());
        let mut b = pool.try_acquire().unwrap();
        let ty = b.leaves().ty(OwnedType::unknown());
        let mut value = SnapshotValue::Bool(true);
        for _ in 0..64 {
            let list = b.list(ty, [value, value].into_iter(), |_, v| v);
            value = object(&mut b, list);
        }
        let snapshot = b.finish(value, &mut Shaper::new(ShapePolicy::Whole));
        assert_eq!(snapshot.root_blob().logical_bytes_v1(), None);
    }
}
