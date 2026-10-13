use std::mem::size_of;

use baml_type::{DeclarationName, Name, Package};
use num_bigint::BigInt;

use crate::{OwnedType, Snapshot, TypeIdentity, pool::Storage, walk::bigint_limb_bytes};

impl Snapshot {
    /// Allocation bytes this snapshot retains, including its owner, arena
    /// capacities, and heap-backed leaves. Shared backing is charged in full
    /// for every reference. The shared pool infrastructure and idle storage,
    /// allocator metadata, and transient encoding buffers are excluded.
    ///
    /// Conservative except for bigints, as values or as type literals:
    /// `num_bigint::BigInt` hides its spare capacity, so one is charged its
    /// limbs. Saturates on overflow.
    pub fn retained_bytes(&self) -> usize {
        let storage: &Storage = &self.0;
        let graph = &storage.graph;
        let leaves = graph
            .strings
            .iter()
            .chain(graph.labels.iter())
            .map(string_bytes)
            .chain(graph.fields.iter().map(|entry| string_bytes(&entry.key)))
            .chain(graph.bigints.iter().map(|bigint| {
                // The shared allocation: its two counts and the bigint.
                (2 * size_of::<usize>() + size_of::<BigInt>())
                    .saturating_add(bigint_limb_bytes(&bigint.value))
            }))
            .chain(graph.names.iter().map(declaration_bytes))
            .chain(graph.types.iter().map(|ty| type_bytes(&ty.ty)));
        leaves.fold(
            size_of::<Self>().saturating_add(storage.capacity_bytes()),
            usize::saturating_add,
        )
    }
}

impl Snapshot {
    /// Bytes the capture's items occupy in its arenas, without spare capacity
    /// or the leaves' own backing.
    pub fn live_arena_bytes(&self) -> usize {
        let graph = &self.0.graph;
        let shape = &self.0.shape;
        size_of_val(&*graph.objects)
            + size_of_val(&*graph.values)
            + size_of_val(&*graph.entries)
            + size_of_val(&*graph.fields)
            + size_of_val(&*graph.bytes)
            + size_of_val(&*graph.strings)
            + size_of_val(&*graph.labels)
            + size_of_val(&*graph.bigints)
            + size_of_val(&*graph.types)
            + size_of_val(&*graph.names)
            + shape.live_bytes()
    }
}

/// A string's backing, which reports `None` only when its size overflows.
fn string_bytes(text: &bex_str::BexStr) -> usize {
    text.retained_heap_bytes().unwrap_or(usize::MAX)
}

fn array_bytes<T>(capacity: usize) -> usize {
    capacity.saturating_mul(size_of::<T>())
}

fn name_bytes(name: &Name) -> usize {
    // SmolStr 0.3 stores heap names in a tight Arc<str>. Allow its two
    // reference counts and trailing alignment padding; inline/static names
    // can safely be charged the same amount.
    name.len()
        .saturating_add(2 * size_of::<usize>())
        .saturating_add(std::mem::align_of::<usize>() - 1)
}

fn declaration_bytes(name: &DeclarationName) -> usize {
    match name {
        DeclarationName::Anonymous(name) => name_bytes(name),
        DeclarationName::Declared(name) => {
            let package = if let Package::Dep(package) = name.key() {
                name_bytes(package)
            } else {
                0
            };
            name.namespace().iter().map(name_bytes).fold(
                array_bytes::<Name>(name.namespace().capacity())
                    .saturating_add(name_bytes(name.name()))
                    .saturating_add(package),
                usize::saturating_add,
            )
        }
    }
}

fn identity_bytes(identity: &TypeIdentity) -> usize {
    match identity {
        TypeIdentity::Resolved(name) => declaration_bytes(name.name()),
        TypeIdentity::Unresolved(_) => 0,
    }
}

fn type_bytes(root: &OwnedType) -> usize {
    let mut bytes = 0usize;
    let mut add = |more: usize| bytes = bytes.saturating_add(more);
    let mut pending = vec![root];
    while let Some(ty) = pending.pop() {
        use baml_type::RealizedTy::{
            Bigint, Bool, Class, Enum, EnumVariant, Float, Function, Future, Int, Interface, List,
            Literal, Map, Media, Never, Null, PromptAst, Resource, RustType, String, Type,
            TypeAlias, Uint8Array, Union, Unknown,
        };
        match ty {
            Int | Bigint | Float | String | Bool | Null | Uint8Array | Media(..) | RustType
            | Type | Resource | PromptAst | Unknown | Never => {}
            Literal(literal, ..) => add(match literal {
                baml_type::Literal::Bigint(value) => bigint_limb_bytes(value),
                baml_type::Literal::String(value) | baml_type::Literal::Float(value) => {
                    value.capacity()
                }
                baml_type::Literal::Int(_) | baml_type::Literal::Bool(_) => 0,
            }),
            Enum(identity) | TypeAlias(identity) => add(identity_bytes(identity)),
            EnumVariant(identity, name) => {
                add(identity_bytes(identity));
                add(name_bytes(name));
            }
            Class(identity, args) => {
                add(identity_bytes(identity));
                add(array_bytes::<OwnedType>(args.len()));
                pending.extend(args.iter());
            }
            Interface(identity, args, bindings) => {
                add(identity_bytes(identity));
                add(array_bytes::<OwnedType>(args.len()));
                add(array_bytes::<(Name, OwnedType)>(bindings.len()));
                pending.extend(args.iter());
                for (name, ty) in bindings {
                    add(name_bytes(name));
                    pending.push(ty);
                }
            }
            Union(args) => {
                add(array_bytes::<OwnedType>(args.len()));
                pending.extend(args.iter());
            }
            List(inner) => {
                add(size_of::<OwnedType>());
                pending.push(inner);
            }
            Map {
                key: left,
                value: right,
                ..
            }
            | Future(left, right) => {
                add(array_bytes::<OwnedType>(2));
                pending.extend([left.as_ref(), right.as_ref()]);
            }
            Function {
                params,
                ret,
                throws,
                ..
            } => {
                add(array_bytes::<
                    baml_type::RealizedFunctionParamTy<TypeIdentity>,
                >(params.len()));
                add(array_bytes::<OwnedType>(2));
                pending.extend([ret.as_ref(), throws.as_ref()]);
                for param in params {
                    if let Some(name) = &param.name {
                        add(name_bytes(name));
                    }
                    pending.push(&param.ty);
                }
            }
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Limits, SnapshotPool, SnapshotValue};

    fn scalar(pool: &SnapshotPool) -> Snapshot {
        pool.try_acquire()
            .unwrap()
            .finish(SnapshotValue::Null, &mut crate::Shaper::default())
    }

    #[test]
    fn accounts_unused_arena_capacity() {
        let pool = SnapshotPool::new(1, Limits::default());
        let mut snapshot = scalar(&pool);
        let before = snapshot.retained_bytes();
        let Storage { graph, meter, .. } = &mut *snapshot.0;
        let old_capacity = graph.bytes.capacity_bytes();
        graph.bytes.reserve(old_capacity + 4096, meter);
        let grown = graph.bytes.capacity_bytes() - old_capacity;
        assert!(grown >= 4096);
        assert_eq!(snapshot.retained_bytes() - before, grown);
    }

    #[test]
    fn accounts_full_shared_slice_parents_for_values_and_keys() {
        let pool = SnapshotPool::new(1, Limits::default());
        let source = bex_str::BexStr::from("a".repeat(100_000));
        let slice = source.substring(0, 100);
        let mut snapshot = scalar(&pool);
        let Storage { graph, meter, .. } = &mut *snapshot.0;
        graph.strings.reserve(1, meter);
        graph.fields.reserve(1, meter);
        let before = snapshot.retained_bytes();
        let Storage { graph, meter, .. } = &mut *snapshot.0;
        graph.strings.push(slice.clone(), meter);
        graph.fields.push(
            crate::FieldEntry {
                key: slice,
                value: SnapshotValue::Null,
            },
            meter,
        );
        assert_eq!(
            snapshot.retained_bytes() - before,
            2 * string_bytes(&source),
        );
    }

    #[test]
    fn a_bigint_is_charged_its_limbs_as_a_value_and_as_a_type_literal() {
        let pool = SnapshotPool::new(1, Limits::default());
        let before = scalar(&pool).retained_bytes();
        // 200 bits: four limbs of eight bytes.
        let big = num_bigint::BigInt::from(1) << 199_u32;
        let mut snapshot = scalar(&pool);
        let Storage { graph, meter, .. } = &mut *snapshot.0;
        graph.bigints.reserve(1, meter);
        let reserved = snapshot.retained_bytes();
        assert!(reserved > before);
        let Storage { graph, meter, .. } = &mut *snapshot.0;
        graph.bigints.push(
            crate::graph::Bigint {
                digest: crate::hash::bigint(&big),
                value: std::sync::Arc::new(big.clone()),
            },
            meter,
        );
        assert_eq!(
            snapshot.retained_bytes() - reserved,
            2 * size_of::<usize>() + size_of::<BigInt>() + 32,
        );
        let literal = OwnedType::Literal(
            baml_type::Literal::Bigint(big),
            baml_type::Freshness::Regular,
        );
        assert_eq!(
            type_bytes(&OwnedType::list(literal)),
            size_of::<OwnedType>() + 32
        );
    }

    #[test]
    fn accounts_nested_type_boxes_and_literal_capacity() {
        let mut value = String::with_capacity(4096);
        value.push('a');
        let capacity = value.capacity();
        let ty = OwnedType::list(OwnedType::Literal(
            baml_type::Literal::String(value),
            baml_type::Freshness::Regular,
        ));
        assert_eq!(type_bytes(&ty), size_of::<OwnedType>() + capacity);
    }

    #[test]
    fn declaration_namespace_spare_capacity_and_long_names_are_charged() {
        let mut namespace = Vec::with_capacity(100);
        namespace.push(Name::new("n".repeat(1000)));
        let capacity = namespace.capacity();
        let declaration = DeclarationName::Declared(baml_type::TypeName::new(
            Name::new("p".repeat(1000)),
            namespace,
            Name::new("t".repeat(1000)),
        ));
        assert!(declaration_bytes(&declaration) >= capacity * size_of::<Name>() + 3000);
        let pool = SnapshotPool::new(1, Limits::default());
        let mut snapshot = scalar(&pool);
        let Storage { graph, meter, .. } = &mut *snapshot.0;
        graph.names.reserve(1, meter);
        let before = snapshot.retained_bytes();
        let charge = declaration_bytes(&declaration);
        let Storage { graph, meter, .. } = &mut *snapshot.0;
        graph.names.push(declaration, meter);
        assert_eq!(snapshot.retained_bytes() - before, charge);
    }

    #[test]
    fn an_array_size_saturates() {
        assert_eq!(array_bytes::<u64>(usize::MAX), usize::MAX);
        assert_eq!(array_bytes::<u8>(usize::MAX), usize::MAX);
        assert_eq!(array_bytes::<u64>(3), 24);
    }
}
