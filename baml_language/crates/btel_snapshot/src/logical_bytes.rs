//! Customer content size, independent of encoding and CAS storage splitting.
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    Blob, MediaSource, SnapshotObject, SnapshotRoot, SnapshotValue, shape::LogicalBytesApproxV1,
};

impl Blob<'_> {
    /// Approximate customer bytes, including content referenced in child CAS blobs.
    ///
    /// Each captured value has an eight-byte type ID, then: null has no payload;
    /// bool one byte; int/float eight; strings UTF-8 bytes; bigint magnitude bytes;
    /// binary captured bytes; enum an eight-byte variant ID. Class fields add an
    /// eight-byte field ID and the full value cost; captured generic args add their
    /// type costs. Lists/maps add their captured declared element/key/value type
    /// costs and every full child value cost. Reflected Type values add the cost of
    /// the represented type. Thus null=8, List<int>[7,7]=48, {a:7}:map<string,int>=49.
    ///
    /// Captured type trees count eight per node plus constituent/generic types.
    /// Interfaces add eight per associated-binding ID; functions count parameter,
    /// return and throws types, excluding parameter names/modes. Literal types add
    /// their scalar payload (no extra type prefix); enum-variant types add eight
    /// for the variant ID. Nominal/alias leaves do not expand unavailable schemas.
    /// Declaration names/source, omitted/truncated content and internal wrappers
    /// add no bytes. This is a semantic approximation, not an encoded byte length.
    ///
    /// Every repeated reference counts full content again; a cycle backedge counts
    /// only its 16-byte CAS ID. Media adds decoded bytes and captured URL/path/MIME
    /// text; invalid base64 adds captured text. None means the expanded size exceeds
    /// u64. Traversal is iterative; closed components, including cyclic ones, are
    /// memoized only when no member of that component is active on the path.
    pub fn logical_bytes_approx_v1(&self) -> Option<u64> {
        enum Task {
            Value(SnapshotValue),
            Finish(crate::ObjectId, u64),
        }
        fn type_bytes(root: &crate::OwnedType) -> Option<u64> {
            use baml_type::RealizedTy;

            let mut pending = vec![root];
            let mut total = 0u64;
            while let Some(ty) = pending.pop() {
                let payload = match ty {
                    RealizedTy::Class(_, args) | RealizedTy::Union(args) => {
                        pending.extend(args.iter());
                        0
                    }
                    RealizedTy::Interface(_, args, bindings) => {
                        pending.extend(args.iter());
                        pending.extend(bindings.iter().map(|(_, ty)| ty));
                        8u64.checked_mul(bindings.len() as u64)?
                    }
                    RealizedTy::List(inner) => {
                        pending.push(inner);
                        0
                    }
                    RealizedTy::Map { key, value } | RealizedTy::Future(key, value) => {
                        pending.extend([key.as_ref(), value.as_ref()]);
                        0
                    }
                    RealizedTy::Function {
                        params,
                        ret,
                        throws,
                    } => {
                        pending.extend(params.iter().map(|param| &param.ty));
                        pending.extend([ret.as_ref(), throws.as_ref()]);
                        0
                    }
                    RealizedTy::Literal(literal, _) => match literal {
                        baml_type::Literal::Int(_) | baml_type::Literal::Float(_) => 8,
                        baml_type::Literal::Bool(_) => 1,
                        baml_type::Literal::String(value) => value.len() as u64,
                        baml_type::Literal::Bigint(value) => value.bits().div_ceil(8),
                    },
                    RealizedTy::EnumVariant(_, _) => 8,
                    RealizedTy::Int
                    | RealizedTy::Bigint
                    | RealizedTy::Float
                    | RealizedTy::String
                    | RealizedTy::Bool
                    | RealizedTy::Null
                    | RealizedTy::Uint8Array
                    | RealizedTy::Media(_)
                    | RealizedTy::Enum(_)
                    | RealizedTy::RustType
                    | RealizedTy::Type
                    | RealizedTy::Resource
                    | RealizedTy::PromptAst
                    | RealizedTy::Void
                    | RealizedTy::TypeAlias(_)
                    | RealizedTy::Unknown
                    | RealizedTy::Never => 0,
                };
                total = total.checked_add(8)?.checked_add(payload)?;
            }
            Some(total)
        }
        fn media_bytes(text: &str) -> u64 {
            // Stream decoding into a sink: no allocation of a second payload.
            let mut decoded = base64::read::DecoderReader::new(
                text.as_bytes(),
                &base64::engine::general_purpose::STANDARD,
            );
            std::io::copy(&mut decoded, &mut std::io::sink()).unwrap_or(text.len() as u64)
        }
        if let LogicalBytesApproxV1::Measured(size) = self.entry().logical_bytes_approx_v1 {
            return size;
        }
        let snapshot = self.snapshot;
        let local_units;
        let units = if snapshot.0.shape.logical_units.is_empty() && snapshot.object_count() > 0 {
            local_units = crate::shape::logical_units(&snapshot.0.graph, self.entry().root);
            &local_units[..]
        } else {
            &snapshot.0.shape.logical_units[..]
        };
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
        let mut active_units = FxHashMap::<u32, usize>::default();
        let mut total = 0u64;
        while let Some(task) = pending.pop() {
            let value = match task {
                Task::Value(value) => value,
                Task::Finish(id, before) => {
                    active.remove(&id);
                    let count = active_units
                        .get_mut(&units[id.0 as usize])
                        .expect("active component");
                    *count -= 1;
                    if *count == 0 {
                        completed.insert(id, total - before);
                    }
                    continue;
                }
            };
            let bytes = match value {
                SnapshotValue::OmittedArg | SnapshotValue::Truncated(_) => 0,
                SnapshotValue::Null => 8,
                SnapshotValue::Bool(_) => 8 + 1,
                SnapshotValue::Int(_) | SnapshotValue::Float(_) => 8 + 8,
                SnapshotValue::String(id) => 8 + snapshot.string(id).len() as u64,
                SnapshotValue::Bigint(id) => 8 + snapshot.bigint(id).bits().div_ceil(8),
                SnapshotValue::Enum { .. } => 8 + 8,
                SnapshotValue::Type(id) => 8u64.checked_add(type_bytes(snapshot.ty(id))?)?,
                SnapshotValue::Object(id) if active.contains(&id) => {
                    std::mem::size_of::<crate::CasId>() as u64
                }
                SnapshotValue::Object(id)
                    if completed.contains_key(&id)
                        && active_units
                            .get(&units[id.0 as usize])
                            .copied()
                            .unwrap_or(0)
                            == 0 =>
                {
                    completed[&id]
                }
                SnapshotValue::Object(id) => {
                    // Child blobs were measured first. Their root total is
                    // reusable; a non-root member of a cycle has its own walk.
                    if let Some(home) = snapshot.0.shape.object_home(id)
                        && home.blob != self.index()
                        && home.node == 0
                        && active_units
                            .get(&units[id.0 as usize])
                            .copied()
                            .unwrap_or(0)
                            == 0
                        && let LogicalBytesApproxV1::Measured(size) =
                            snapshot.0.shape.blobs[home.blob.0 as usize].logical_bytes_approx_v1
                    {
                        total = total.checked_add(size?)?;
                        continue;
                    }
                    active.insert(id);
                    *active_units.entry(units[id.0 as usize]).or_default() += 1;
                    pending.push(Task::Finish(id, total));
                    match snapshot.object(id) {
                        SnapshotObject::Uint8Array { data } => 8 + data.len() as u64,
                        SnapshotObject::List {
                            element_type,
                            items,
                            ..
                        } => {
                            pending
                                .extend(snapshot.values(*items).iter().copied().map(Task::Value));
                            8u64.checked_add(type_bytes(snapshot.ty(*element_type))?)?
                        }
                        SnapshotObject::Map {
                            key_type,
                            value_type,
                            entries,
                            ..
                        } => {
                            for entry in snapshot.entries(*entries) {
                                pending.extend([entry.key, entry.value].map(Task::Value));
                            }
                            8u64.checked_add(type_bytes(snapshot.ty(*key_type))?)?
                                .checked_add(type_bytes(snapshot.ty(*value_type))?)?
                        }
                        SnapshotObject::Instance {
                            type_arguments,
                            fields,
                            ..
                        } => {
                            let mut bytes = 8u64;
                            for ty in snapshot.type_arguments(*type_arguments) {
                                bytes = bytes.checked_add(type_bytes(ty)?)?;
                            }
                            for field in snapshot.fields(*fields) {
                                bytes = bytes.checked_add(8)?;
                                pending.push(Task::Value(field.value));
                            }
                            bytes
                        }
                        SnapshotObject::Cell(value) => {
                            // Internal wrapper; its value carries the type ID.
                            pending.push(Task::Value(*value));
                            0
                        }
                        SnapshotObject::Media {
                            mime_type, source, ..
                        } => {
                            let metadata =
                                mime_type.map_or(0, |id| snapshot.label(id).len() as u64);
                            8 + metadata
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
    fn exact_bytes_include_value_type_ids_but_exclude_uncaptured_content() {
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
        let declaration = b.declaration(
            &baml_type::DeclarationName::Declared(baml_type::TypeName::from_dotted_path(
                "app.Kind",
            )),
            baml_type::typetag::TypeTag::of_static_index(0),
            true,
        );
        let declaration = b.leaves().object(declaration).unwrap();
        let name = b.leaves().label(&"LongVariantName".into()).unwrap();
        let enum_value = SnapshotValue::Enum {
            declaration,
            variant: 0,
            name,
        };
        let class = b.declaration(
            &baml_type::DeclarationName::Declared(baml_type::TypeName::from_dotted_path(
                "app.Class",
            )),
            baml_type::typetag::TypeTag::of_static_index(1),
            false,
        );
        let class = b.leaves().object(class).unwrap();
        let instance = b.instance(
            class,
            [],
            [("é", SnapshotValue::Bool(true))].into_iter(),
            |_, (key, value)| (key.into(), value),
        );
        let instance = object(&mut b, instance);
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
                enum_value,
                instance,
            ]
            .into_iter(),
            |_, value| value,
        );
        let snapshot = b.finish(root, &mut Shaper::default());
        assert_eq!(
            snapshot.root_blob().logical_bytes_approx_v1(),
            Some(8 + 9 + 16 + 16 + 10 + 10 + 11 + 16 + 16 + 8 + 8 + 9)
        );
    }

    #[test]
    fn containers_and_reflected_types_count_captured_dynamic_type_content() {
        let pool = SnapshotPool::new(1, Limits::default());
        let check = |make: fn(&mut Builder) -> SnapshotValue, expected| {
            let mut b = pool.try_acquire().unwrap();
            let root = make(&mut b);
            let snapshot = b.finish(root, &mut Shaper::default());
            assert_eq!(
                snapshot.root_blob().logical_bytes_approx_v1(),
                Some(expected)
            );
        };
        check(|_| SnapshotValue::Null, 8);
        check(
            |b| {
                let ty = b.leaves().ty(OwnedType::int());
                let list = b.list(ty, [SnapshotValue::Int(7); 2].into_iter(), |_, v| v);
                object(b, list)
            },
            48,
        );
        check(
            |b| {
                let key = b.leaves().ty(OwnedType::string());
                let value = b.leaves().ty(OwnedType::int());
                let map = b.map(key, value, [()].into_iter(), |leaves, ()| {
                    (leaves.string_value(&"a".into()), SnapshotValue::Int(7))
                });
                object(b, map)
            },
            49,
        );
        check(
            |b| {
                let class = b.declaration(
                    &baml_type::DeclarationName::Declared(baml_type::TypeName::from_dotted_path(
                        "app.Example",
                    )),
                    baml_type::typetag::TypeTag::of_static_index(0),
                    false,
                );
                let class = b.leaves().object(class).unwrap();
                let instance = b.instance(class, [], [()].into_iter(), |_, ()| {
                    ("long_field_name".into(), SnapshotValue::Int(7))
                });
                object(b, instance)
            },
            32,
        );
        check(
            |b| SnapshotValue::Type(b.leaves().ty(OwnedType::list(OwnedType::int()))),
            24,
        );
        check(
            |b| {
                let element_type = b.leaves().ty(OwnedType::Union(
                    vec![OwnedType::int(), OwnedType::string()].into_boxed_slice(),
                ));
                let text = b.leaves().string_value(&"hi".into());
                let list = b.list(
                    element_type,
                    [SnapshotValue::Int(7), text].into_iter(),
                    |_, v| v,
                );
                object(b, list)
            },
            58,
        );
        check(
            |b| {
                let ty = OwnedType::Map {
                    key: Box::new(OwnedType::string()),
                    value: Box::new(OwnedType::list(OwnedType::Union(
                        vec![OwnedType::int(), OwnedType::Null].into_boxed_slice(),
                    ))),
                };
                SnapshotValue::Type(b.leaves().ty(ty))
            },
            56,
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
            assert_eq!(
                snapshot.root_blob().logical_bytes_approx_v1(),
                Some(24 + 10 + 16 + 10 + 11 + 10 + 16 + 10 + 11)
            );
            let root_id = snapshot.root_id();
            let split = snapshot.split();
            // Delivery can drop strings before it reads the parent metadata.
            drop(split.leaves);
            let structure = split.structure.unwrap();
            let root = structure.blobs().find(|blob| blob.id() == root_id).unwrap();
            assert_eq!(root.logical_bytes_approx_v1(), Some(118));
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
        assert_eq!(
            snapshot.root_blob().logical_bytes_approx_v1(),
            Some((16 + 16 + 16) * 2)
        );
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
                snapshot.root_blob().logical_bytes_approx_v1(),
                Some((16 + 16 + 16 + 9 + 16) * 2)
            );
            let split = snapshot.split();
            assert_eq!(
                split
                    .structure
                    .unwrap()
                    .blobs()
                    .last()
                    .unwrap()
                    .logical_bytes_approx_v1(),
                Some(146)
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
        assert_eq!(
            snapshot.root_blob().logical_bytes_approx_v1(),
            Some(8 + 9 + 4)
        );
        let split = snapshot.split();
        drop(split.leaves);
        assert_eq!(
            split
                .structure
                .unwrap()
                .blobs()
                .last()
                .unwrap()
                .logical_bytes_approx_v1(),
            Some(21)
        );
    }

    #[test]
    fn repeated_cyclic_subgraph_is_memoized_and_reports_real_overflow() {
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
        let mut value = SnapshotValue::Object(id);
        for _ in 0..60 {
            let list = b.list(ty, [value, value].into_iter(), |_, v| v);
            value = object(&mut b, list);
        }
        // At least 48 * 2^60 does not fit u64; the physical graph has only 61 objects.
        let snapshot = b.finish(value, &mut Shaper::new(ShapePolicy::Whole));
        assert_eq!(snapshot.root_blob().logical_bytes_approx_v1(), None);
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
        assert_eq!(snapshot.root_blob().logical_bytes_approx_v1(), None);
    }
}
