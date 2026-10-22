//! Bounded, owned host observations supplied by adapters without invoking user
//! serializers, getters, iterators or display hooks. No live host references.
use crate::{
    Builder, Limit, OwnedType, Shaper, Snapshot, SnapshotObject, SnapshotValue, TypeIdentity,
};

/// A host supplies spellings; the engine resolves them to real declarations.
pub type HostType = baml_type::RealizedTy<String>;

#[derive(Clone, Debug)]
pub struct HostDeclaration {
    pub identity: baml_type::TaggedTypeName,
    pub fields: Vec<String>,
    pub variants: Option<Vec<String>>,
}

#[derive(Debug)]
pub enum HostValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<Self>),
    Map(Vec<(String, Self)>),
    Instance {
        name: String,
        type_args: Vec<HostType>,
        fields: Vec<(String, Self)>,
    },
    Enum {
        name: String,
        variant: String,
    },
    Unavailable,
    Truncated(Limit),
}

/// Adapter traversal must obey these bounds before allocating this tree. The
/// snapshot builder independently enforces its existing transport budgets.
pub const MAX_DEPTH: usize = 8;
pub const MAX_VALUES: usize = 512;
pub const MAX_BYTES: usize = 64 * 1024;

pub fn capture(builder: Builder, value: &HostValue) -> Snapshot {
    capture_with(builder, value, |_| None)
}

pub fn capture_with<'a>(
    mut builder: Builder,
    value: &HostValue,
    resolve: impl Fn(&str) -> Option<&'a HostDeclaration>,
) -> Snapshot {
    let mut remaining = MAX_VALUES;
    let mut bytes = MAX_BYTES;
    let value = add(&mut builder, value, 0, &mut remaining, &mut bytes, &resolve);
    // Host observations are small and arrive from adapter threads, which
    // hold no shaping scratch of their own.
    builder.finish(value, &mut Shaper::default())
}

fn add<'a>(
    builder: &mut Builder,
    value: &HostValue,
    depth: usize,
    remaining: &mut usize,
    bytes: &mut usize,
    resolve: &impl Fn(&str) -> Option<&'a HostDeclaration>,
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
                copied.push(add(builder, value, depth + 1, remaining, bytes, resolve));
            }
            let element_type = builder.leaves().ty(OwnedType::Unknown);
            let list = builder.list(element_type, copied.into_iter(), |_, value| value);
            object(builder, list)
        }
        HostValue::Map(entries)
        | HostValue::Instance {
            fields: entries, ..
        } => {
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
            let declared_fields = match value {
                HostValue::Instance { name, .. } => resolve(name).map(|d| &d.fields),
                _ => None,
            };
            let mut values = Vec::with_capacity(entries.len());
            for (key, value) in entries {
                if declared_fields.is_some_and(|fields| !fields.contains(key)) {
                    continue;
                }
                if *remaining == 0 {
                    return SnapshotValue::Truncated(Limit::Values);
                }
                values.push((
                    key,
                    add(builder, value, depth + 1, remaining, bytes, resolve),
                ));
            }
            if let HostValue::Instance {
                name, type_args, ..
            } = value
            {
                if name.len() > *bytes {
                    return SnapshotValue::Truncated(Limit::Bytes);
                }
                *bytes -= name.len();
                let Some(declaration) = resolve(name).filter(|d| d.variants.is_none()) else {
                    return object(builder, SnapshotObject::NonSnapshotableValue {});
                };
                let args = type_args
                    .iter()
                    .map(|ty| resolve_type(ty, depth + 1, remaining, bytes, resolve))
                    .collect::<Result<Vec<_>, _>>();
                let args = match args {
                    Ok(args) => args,
                    Err(Some(limit)) => return SnapshotValue::Truncated(limit),
                    Err(None) => return object(builder, SnapshotObject::NonSnapshotableValue {}),
                };
                let declaration = builder.declaration(
                    declaration.identity.name(),
                    declaration.identity.tag(),
                    false,
                    // A host value has no recorded definition at hand: named by tag.
                    None,
                );
                let Some(declaration) = builder.leaves().object(declaration) else {
                    return SnapshotValue::Truncated(Limit::Objects);
                };
                let instance =
                    builder.instance(declaration, args, values.into_iter(), |_, (key, value)| {
                        (key.as_str().into(), value)
                    });
                return object(builder, instance);
            }
            let key_type = builder.leaves().ty(OwnedType::string());
            let value_type = builder.leaves().ty(OwnedType::Unknown);
            let map = builder.map(
                key_type,
                value_type,
                values.into_iter(),
                |leaves, (key, value)| (leaves.string_value(&key.as_str().into()), value),
            );
            object(builder, map)
        }
        HostValue::Enum { name, variant } => {
            if name.len().saturating_add(variant.len()) > *bytes {
                return SnapshotValue::Truncated(Limit::Bytes);
            }
            *bytes -= name.len() + variant.len();
            let Some(declaration) = resolve(name) else {
                return object(builder, SnapshotObject::NonSnapshotableValue {});
            };
            let Some(index) = declaration
                .variants
                .as_ref()
                .and_then(|v| v.iter().position(|v| v == variant))
            else {
                return object(builder, SnapshotObject::NonSnapshotableValue {});
            };
            let declared = builder.declaration(
                declaration.identity.name(),
                declaration.identity.tag(),
                true,
                // A host value has no recorded definition at hand: named by tag.
                None,
            );
            let Some(declaration) = builder.leaves().object(declared) else {
                return SnapshotValue::Truncated(Limit::Objects);
            };
            let Some(name) = builder.leaves().label(&variant.as_str().into()) else {
                return SnapshotValue::Truncated(Limit::Bytes);
            };
            SnapshotValue::Enum {
                declaration,
                variant: u32::try_from(index).expect("bounded enum variants"),
                name,
            }
        }
    }
}

/// Resolve only the host metadata vocabulary, charging every nested type node.
/// This also bounds observations passed directly to the engine by Rust callers.
fn resolve_type<'a>(
    ty: &HostType,
    depth: usize,
    remaining: &mut usize,
    bytes: &mut usize,
    resolve: &impl Fn(&str) -> Option<&'a HostDeclaration>,
) -> Result<OwnedType, Option<Limit>> {
    // Adapters end a type nested past MAX_DEPTH with one `Unknown`.
    if depth > MAX_DEPTH && (depth > MAX_DEPTH + 1 || !matches!(ty, HostType::Unknown)) {
        return Err(Some(Limit::Depth));
    }
    if *remaining == 0 {
        return Err(Some(Limit::Values));
    }
    *remaining -= 1;
    let mut head = |name: &str, is_enum: bool| {
        if name.len() > *bytes {
            return Err(Some(Limit::Bytes));
        }
        *bytes -= name.len();
        resolve(name)
            .filter(|d| d.variants.is_some() == is_enum)
            .map(|d| TypeIdentity::Resolved(d.identity.clone()))
            .ok_or(None)
    };
    Ok(match ty {
        HostType::Int => OwnedType::Int,
        HostType::Float => OwnedType::Float,
        HostType::Bool => OwnedType::Bool,
        HostType::String => OwnedType::String,
        HostType::Null => OwnedType::Null,
        HostType::List(inner) => OwnedType::List(Box::new(resolve_type(
            inner,
            depth + 1,
            remaining,
            bytes,
            resolve,
        )?)),
        HostType::Map { key, value } => OwnedType::Map {
            key: Box::new(resolve_type(key, depth + 1, remaining, bytes, resolve)?),
            value: Box::new(resolve_type(value, depth + 1, remaining, bytes, resolve)?),
        },
        HostType::Union(options) => OwnedType::Union(
            options
                .iter()
                .map(|ty| resolve_type(ty, depth + 1, remaining, bytes, resolve))
                .collect::<Result<Vec<_>, _>>()?
                .into(),
        ),
        HostType::Class(name, args) => {
            let identity = head(name, false)?;
            let args = args
                .iter()
                .map(|ty| resolve_type(ty, depth + 1, remaining, bytes, resolve))
                .collect::<Result<Vec<_>, _>>()?;
            OwnedType::Class(identity, args.into())
        }
        HostType::Enum(name) => OwnedType::Enum(head(name, true)?),
        _ => OwnedType::Unknown,
    })
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

    fn declaration(name: &str, index: usize, variants: Option<Vec<String>>) -> HostDeclaration {
        HostDeclaration {
            identity: baml_type::TaggedTypeName::new(
                baml_type::typetag::TypeTag::of_static_index(index),
                baml_type::DeclarationName::Declared(baml_type::TypeName::from_dotted_path(name)),
            ),
            fields: vec!["value".into()],
            variants,
        }
    }

    #[test]
    fn host_nominal_values_require_engine_declarations_and_bound_type_metadata() {
        let pool = SnapshotPool::new(1, Limits::default());
        let class = declaration("user.Box", 3, None);
        let value = HostValue::Instance {
            name: "user.Box".into(),
            type_args: vec![HostType::Int],
            fields: vec![("value".into(), HostValue::Int(7))],
        };
        let snapshot = capture(pool.try_acquire().unwrap(), &value);
        let SnapshotRoot::Value(SnapshotValue::Object(id)) = snapshot.root() else {
            panic!("opaque")
        };
        assert!(matches!(
            snapshot.object(id),
            SnapshotObject::NonSnapshotableValue {}
        ));
        drop(snapshot);
        let snapshot = capture_with(pool.try_acquire().unwrap(), &value, |name| {
            (name == "user.Box").then_some(&class)
        });
        let mut blob = vec![];
        snapshot
            .root_blob()
            .write(&mut crate::BlobScratch::default(), &mut blob)
            .unwrap();
        let decoded = crate::decode_blob(&blob, &DecodeLimits::default()).unwrap();
        let crate::DecodedRoot::Value(crate::DecodedValue::Object(id)) = decoded.root else {
            panic!("instance")
        };
        let crate::DecodedObject::Instance {
            declaration,
            type_arguments,
            ..
        } = decoded.object(id)
        else {
            panic!("instance")
        };
        assert_eq!(type_arguments[0].decoded.as_deref(), Some(&OwnedType::Int));
        assert!(
            matches!(decoded.object(*declaration), crate::DecodedObject::Declaration { tag, .. } if *tag == Some(class.identity.tag()))
        );
        drop(snapshot);
        let mut deep = HostType::Int;
        for _ in 0..=MAX_DEPTH {
            deep = HostType::List(Box::new(deep));
        }
        let snapshot = capture_with(
            pool.try_acquire().unwrap(),
            &HostValue::Instance {
                name: "user.Box".into(),
                type_args: vec![deep],
                fields: vec![],
            },
            |_| Some(&class),
        );
        assert!(matches!(
            snapshot.root(),
            SnapshotRoot::Value(SnapshotValue::Truncated(Limit::Depth))
        ));
        drop(snapshot);
        // Adapters end a type nested past the limit with one Unknown.
        let mut deep = HostType::Unknown;
        for _ in 0..MAX_DEPTH {
            deep = HostType::List(Box::new(deep));
        }
        let snapshot = capture_with(
            pool.try_acquire().unwrap(),
            &HostValue::Instance {
                name: "user.Box".into(),
                type_args: vec![deep],
                fields: vec![],
            },
            |_| Some(&class),
        );
        let SnapshotRoot::Value(SnapshotValue::Object(id)) = snapshot.root() else {
            panic!("instance")
        };
        assert!(matches!(
            snapshot.object(id),
            SnapshotObject::Instance { .. }
        ));
    }

    #[test]
    fn host_enum_uses_the_declared_variant_index_and_round_trips() {
        let pool = SnapshotPool::new(1, Limits::default());
        let declared = declaration("user.Mood", 4, Some(vec!["SAD".into(), "HAPPY".into()]));
        let value = HostValue::Enum {
            name: "user.Mood".into(),
            variant: "HAPPY".into(),
        };
        let snapshot = capture_with(pool.try_acquire().unwrap(), &value, |_| Some(&declared));
        assert!(matches!(
            snapshot.root(),
            SnapshotRoot::Value(SnapshotValue::Enum { variant: 1, .. })
        ));
        let mut blob = vec![];
        snapshot
            .root_blob()
            .write(&mut crate::BlobScratch::default(), &mut blob)
            .unwrap();
        crate::decode_blob(&blob, &DecodeLimits::default()).unwrap();
    }

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
