use std::mem::size_of;

use baml_type::{DeclarationName, Name, Package};

use crate::{OwnedType, Snapshot, SnapshotObject, Storage, TypeIdentity};

impl Snapshot {
    /// Conservative retained allocation bytes for this snapshot, including its
    /// owner, arena capacities, and heap-backed leaves. Shared backing is charged
    /// in full for every reference. The shared pool infrastructure and idle
    /// storage, allocator metadata, and transient encoding buffers are excluded.
    ///
    /// Returns `None` on overflow or when backing cannot be bounded through safe
    /// APIs. In particular, `num_bigint::BigInt` hides its limb capacity, so both
    /// bigint values and bigint type literals are unaccountable. Bounded delivery
    /// must reject these snapshots, not treat `None` as zero or logical size.
    pub fn retained_bytes(&self) -> Option<usize> {
        let s = &self.0;
        if !s.bigints.is_empty() {
            return None;
        }
        let mut bytes = size_of::<Self>().checked_add(size_of::<Storage>())?;
        macro_rules! arena {
            ($($field:ident),+ $(,)?) => {
                $(bytes = bytes.checked_add(capacity_bytes(&s.$field)?)?;)+
            };
        }
        arena!(
            values,
            objects,
            entries,
            bytes,
            strings,
            bigints,
            types,
            string_hashes,
            bigint_hashes,
            type_hashes,
            object_hashes,
        );
        for string in &s.strings {
            bytes = bytes.checked_add(string.retained_heap_bytes()?)?;
        }
        for entry in &s.entries {
            bytes = bytes.checked_add(entry.key.retained_heap_bytes()?)?;
        }
        for object in &s.objects {
            if let SnapshotObject::Declaration { name, .. } = object {
                bytes = bytes.checked_add(declaration_bytes(name)?)?;
            }
        }
        for ty in &s.types {
            bytes = bytes.checked_add(type_bytes(ty)?)?;
        }
        Some(bytes)
    }
}

fn capacity_bytes<T>(values: &Vec<T>) -> Option<usize> {
    array_bytes::<T>(values.capacity())
}

fn array_bytes<T>(capacity: usize) -> Option<usize> {
    capacity.checked_mul(size_of::<T>())
}

fn name_bytes(name: &Name) -> Option<usize> {
    // SmolStr 0.3 stores heap names in a tight Arc<str>. Allow its two
    // reference counts and trailing alignment padding; inline/static names
    // can safely be charged the same amount.
    name.len()
        .checked_add(2 * size_of::<usize>())?
        .checked_add(std::mem::align_of::<usize>() - 1)
}

fn declaration_bytes(name: &DeclarationName) -> Option<usize> {
    match name {
        DeclarationName::Anonymous(name) => name_bytes(name),
        DeclarationName::Declared(name) => {
            let mut bytes =
                capacity_bytes(name.namespace())?.checked_add(name_bytes(name.name())?)?;
            if let Package::Dep(package) = name.key() {
                bytes = bytes.checked_add(name_bytes(package)?)?;
            }
            for segment in name.namespace() {
                bytes = bytes.checked_add(name_bytes(segment)?)?;
            }
            Some(bytes)
        }
    }
}

fn identity_bytes(identity: &TypeIdentity) -> Option<usize> {
    match identity {
        TypeIdentity::Resolved(name) => declaration_bytes(name.name()),
        TypeIdentity::Unresolved(_) => Some(0),
    }
}

fn type_bytes(root: &OwnedType) -> Option<usize> {
    let mut bytes = 0usize;
    let mut pending = vec![root];
    while let Some(ty) = pending.pop() {
        use baml_type::RealizedTy::{
            Bigint, Bool, Class, Enum, EnumVariant, Float, Function, Future, Int, Interface, List,
            Literal, Map, Media, Never, Null, PromptAst, Resource, RustType, String, Type,
            TypeAlias, Uint8Array, Union, Unknown, Void,
        };
        match ty {
            Int | Bigint | Float | String | Bool | Null | Uint8Array | Media(..) | RustType
            | Type | Resource | PromptAst | Void | Unknown | Never => {}
            Literal(literal, ..) => {
                bytes = bytes.checked_add(match literal {
                    baml_type::Literal::Bigint(_) => return None,
                    baml_type::Literal::String(value) | baml_type::Literal::Float(value) => {
                        value.capacity()
                    }
                    baml_type::Literal::Int(_) | baml_type::Literal::Bool(_) => 0,
                })?;
            }
            Enum(identity) | TypeAlias(identity) => {
                bytes = bytes.checked_add(identity_bytes(identity)?)?;
            }
            EnumVariant(identity, name) => {
                bytes = bytes
                    .checked_add(identity_bytes(identity)?)?
                    .checked_add(name_bytes(name)?)?;
            }
            Class(identity, args) => {
                bytes = bytes
                    .checked_add(identity_bytes(identity)?)?
                    .checked_add(args.len().checked_mul(size_of::<OwnedType>())?)?;
                pending.extend(args.iter());
            }
            Interface(identity, args, bindings) => {
                bytes = bytes
                    .checked_add(identity_bytes(identity)?)?
                    .checked_add(args.len().checked_mul(size_of::<OwnedType>())?)?
                    .checked_add(bindings.len().checked_mul(size_of::<(Name, OwnedType)>())?)?;
                pending.extend(args.iter());
                for (name, ty) in bindings {
                    bytes = bytes.checked_add(name_bytes(name)?)?;
                    pending.push(ty);
                }
            }
            Union(args) => {
                bytes = bytes.checked_add(args.len().checked_mul(size_of::<OwnedType>())?)?;
                pending.extend(args.iter());
            }
            List(inner) => {
                bytes = bytes.checked_add(size_of::<OwnedType>())?;
                pending.push(inner);
            }
            Map {
                key: left,
                value: right,
                ..
            }
            | Future(left, right) => {
                bytes = bytes.checked_add(2usize.checked_mul(size_of::<OwnedType>())?)?;
                pending.extend([left.as_ref(), right.as_ref()]);
            }
            Function {
                params,
                ret,
                throws,
                ..
            } => {
                bytes = bytes
                    .checked_add(params.len().checked_mul(size_of::<
                        baml_type::RealizedFunctionParamTy<TypeIdentity>,
                    >())?)?
                    .checked_add(2usize.checked_mul(size_of::<OwnedType>())?)?;
                pending.extend([ret.as_ref(), throws.as_ref()]);
                for param in params {
                    if let Some(name) = &param.name {
                        bytes = bytes.checked_add(name_bytes(name)?)?;
                    }
                    pending.push(&param.ty);
                }
            }
        }
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Limits, SnapshotPool, SnapshotValue};

    fn scalar(pool: &SnapshotPool) -> Snapshot {
        pool.try_acquire()
            .unwrap()
            .finish_value(SnapshotValue::Null)
    }

    #[test]
    fn accounts_unused_arena_capacity() {
        let pool = SnapshotPool::new(1, Limits::default());
        let mut snapshot = scalar(&pool);
        let before = snapshot.retained_bytes().unwrap();
        let old_capacity = snapshot.0.bytes.capacity();
        snapshot.0.bytes.reserve(old_capacity + 4096);
        assert_eq!(
            snapshot.retained_bytes().unwrap() - before,
            snapshot.0.bytes.capacity() - old_capacity,
        );
        assert!(before > snapshot.live_arena_bytes());
    }

    #[test]
    fn accounts_full_shared_slice_parents_for_values_and_keys() {
        let pool = SnapshotPool::new(1, Limits::default());
        let source = bex_str::BexStr::from("a".repeat(100_000));
        let slice = source.substring(0, 100);
        let mut snapshot = scalar(&pool);
        snapshot.0.strings.reserve(1);
        snapshot.0.entries.reserve(1);
        let before = snapshot.retained_bytes().unwrap();
        snapshot.0.strings.push(slice.clone());
        snapshot.0.entries.push(crate::MapEntry {
            key: slice,
            value: SnapshotValue::Null,
        });
        assert_eq!(
            snapshot.retained_bytes().unwrap() - before,
            2 * source.retained_heap_bytes().unwrap(),
        );
    }

    #[test]
    fn refuses_bigints_even_when_logical_value_is_small() {
        let pool = SnapshotPool::new(1, Limits::default());
        let mut builder = pool.try_acquire().unwrap();
        let id = builder
            .bigint(&std::sync::Arc::new(num_bigint::BigInt::from(0)))
            .unwrap();
        assert!(
            builder
                .finish_value(SnapshotValue::Bigint(id))
                .retained_bytes()
                .is_none()
        );
        let literal = OwnedType::Literal(
            baml_type::Literal::Bigint(num_bigint::BigInt::from(0)),
            baml_type::Freshness::Regular,
        );
        assert!(type_bytes(&OwnedType::list(literal)).is_none());
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
        assert_eq!(type_bytes(&ty), Some(size_of::<OwnedType>() + capacity));
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
        assert!(declaration_bytes(&declaration).unwrap() >= capacity * size_of::<Name>() + 3000);
        let pool = SnapshotPool::new(1, Limits::default());
        let mut snapshot = scalar(&pool);
        snapshot.0.objects.reserve(1);
        let before = snapshot.retained_bytes().unwrap();
        let charge = declaration_bytes(&declaration).unwrap();
        snapshot.0.objects.push(SnapshotObject::Declaration {
            name: declaration,
            tag: baml_type::typetag::TypeTag::of_head("test.Declaration"),
            is_enum: false,
        });
        assert_eq!(snapshot.retained_bytes().unwrap() - before, charge);
    }

    #[test]
    fn checked_array_size_rejects_overflow() {
        assert_eq!(array_bytes::<u64>(usize::MAX), None);
        assert_eq!(array_bytes::<u8>(usize::MAX), Some(usize::MAX));
    }
}
