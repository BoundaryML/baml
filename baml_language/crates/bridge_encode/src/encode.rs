//! The type-directed walk.
//!
//! [`encode`] takes one host value and the type its slot declares and
//! returns the exact wire value, the concrete type that value has, and
//! whether the host's language had to convert anything to get there. The
//! walk is one function over the slot's shape; a bridge contributes only the
//! answers its [`HostValue`] gives.
//!
//! Three slot shapes do not fix a node's type: `unknown`, an interface and an
//! unbound type variable (open slots), and a union. A scalar, null, enum or
//! handle under such a slot is self-evident. A list or map is not, so the
//! node carries a `value_type` naming its concrete type, and the engine takes
//! that as the node's type. A class node carries one under every slot, since
//! the wire's `class_value` has no name of its own. Nothing else ever does.

use std::collections::HashMap;

use baml_type::{Literal, MediaKind, Name, ParamTy, RuntimeTy, TypeName};
use bridge_ctypes::{
    baml_bridge::cffi::{
        InboundClassValue, InboundEnumValue, InboundListValue, InboundMapEntry, InboundMapValue,
        InboundValue, inbound_map_entry::Key, inbound_value::Value,
    },
    runtime_ty_to_proto_ty,
};
use num_bigint::BigInt;

use crate::{
    error::{EncodeError, EncodeErrorKind, PathSegment},
    facts::{Facts, FunctionSig, mentions_type_var, substitute, unify},
    host::{Answer, HostValue, Leaf, LeafKind, Scalar, ScalarSlot},
};

/// Whether the host's language converted the value to make it fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    AsIs,
    Converted,
}

impl Fit {
    fn and(self, other: Fit) -> Fit {
        match (self, other) {
            (Fit::AsIs, Fit::AsIs) => Fit::AsIs,
            (Fit::AsIs, Fit::Converted) | (Fit::Converted, _) => Fit::Converted,
        }
    }
}

/// One encoded node.
#[derive(Debug, Clone, PartialEq)]
pub struct Encoded {
    pub value: InboundValue,
    /// The concrete type the node has. Equal to the slot for a realized
    /// concrete slot; the value's own type under an open slot or a union.
    pub ty: RuntimeTy,
    pub fit: Fit,
}

/// Encode a function's arguments against its signature.
///
/// `arguments` are `(declared parameter name, value)`; `type_args` are the
/// caller's explicit type-parameter bindings by name. A type variable the
/// caller did not bind stays open and the value states its type. Names and
/// arity are the engine's to judge: an argument no parameter declares goes
/// as the type its value has, and the engine reports it.
pub fn encode_arguments<V: HostValue>(
    facts: &Facts,
    signature: &FunctionSig,
    type_args: &HashMap<Name, RuntimeTy>,
    arguments: &[(String, V)],
) -> Result<Vec<InboundMapEntry>, EncodeError> {
    let resolve = |param: &ParamTy| type_args.get(&Name::new(param.as_str())).cloned();
    arguments
        .iter()
        .map(|(name, value)| {
            let path = vec![PathSegment::Argument(name.clone())];
            let slot = signature
                .params
                .iter()
                .find(|param| param.name == *name)
                .map_or(RuntimeTy::Unknown, |param| substitute(&param.ty, &resolve));
            let encoded = Walk { facts, path }.encode(value, &slot)?;
            Ok(InboundMapEntry {
                key: Some(Key::StringKey(name.clone())),
                value: Some(encoded.value),
            })
        })
        .collect()
}

/// Encode one value against `slot`, with `path` leading to it.
pub fn encode<V: HostValue>(
    facts: &Facts,
    value: &V,
    slot: &RuntimeTy,
    path: Vec<PathSegment>,
) -> Result<Encoded, EncodeError> {
    Walk { facts, path }.encode(value, slot)
}

struct Walk<'f> {
    facts: &'f Facts,
    path: Vec<PathSegment>,
}

impl Walk<'_> {
    fn child(&self, segment: PathSegment) -> Walk<'_> {
        let mut path = self.path.clone();
        path.push(segment);
        Walk {
            facts: self.facts,
            path,
        }
    }

    fn err(&self, kind: impl Into<Box<EncodeErrorKind>>) -> EncodeError {
        EncodeError {
            path: self.path.clone(),
            kind: kind.into(),
        }
    }

    fn mismatch<V: HostValue>(&self, value: &V, expected: &RuntimeTy) -> EncodeError {
        self.err(EncodeErrorKind::Mismatch {
            expected: expected.clone(),
            found: value.describe(),
        })
    }

    fn encode<V: HostValue>(&self, value: &V, slot: &RuntimeTy) -> Result<Encoded, EncodeError> {
        let slot = self.facts.expand(slot).map_err(|kind| self.err(kind))?;
        if let Some(leaf) = value.leaf() {
            let leaf = leaf.map_err(|error| self.err(EncodeErrorKind::Host(error.0)))?;
            return self.leaf_in(value, leaf, slot);
        }
        match slot {
            RuntimeTy::Unknown | RuntimeTy::Interface(..) | RuntimeTy::TypeVar(_) => {
                self.open(value, slot)
            }
            RuntimeTy::Union(members) => self.union(value, members),
            RuntimeTy::Null => {
                if value.is_null() {
                    Ok(exact(InboundValue::default(), RuntimeTy::Null))
                } else {
                    Err(self.mismatch(value, slot))
                }
            }
            RuntimeTy::Int => self.scalar(value, slot, ScalarSlot::Int),
            RuntimeTy::Bigint => self.scalar(value, slot, ScalarSlot::Bigint),
            RuntimeTy::Float => self.scalar(value, slot, ScalarSlot::Float),
            RuntimeTy::Bool => self.scalar(value, slot, ScalarSlot::Bool),
            RuntimeTy::String => self.scalar(value, slot, ScalarSlot::String),
            RuntimeTy::Uint8Array => self.scalar(value, slot, ScalarSlot::Bytes),
            RuntimeTy::Literal(literal, _) => self.literal(value, slot, literal),
            RuntimeTy::Enum(name) => self.enumeration(value, slot, name, None),
            RuntimeTy::EnumVariant(name, variant) => {
                self.enumeration(value, slot, name, Some(variant.as_str()))
            }
            RuntimeTy::List(inner) => self.list(value, slot, inner),
            RuntimeTy::Map { key, value: item } => self.map(value, slot, key, item),
            RuntimeTy::Class(name, args) => self.class(value, slot, name, args),
            RuntimeTy::Function { .. } => match value.callable(slot) {
                Some(Ok(wire)) => Ok(exact(wire, slot.clone())),
                Some(Err(error)) => Err(self.err(EncodeErrorKind::Host(error.0))),
                None => Err(self.mismatch(value, slot)),
            },
            // Only a host-owned leaf inhabits these, and leaves were handled
            // above: anything else is not a member.
            RuntimeTy::Media(_)
            | RuntimeTy::PromptAst
            | RuntimeTy::Type
            | RuntimeTy::Resource
            | RuntimeTy::RustType
            | RuntimeTy::Future(..)
            | RuntimeTy::Void
            | RuntimeTy::Never
            | RuntimeTy::AssociatedTypeProjection { .. } => Err(self.mismatch(value, slot)),
            RuntimeTy::TypeAlias(_) => unreachable!("expand() leaves no alias at the head"),
        }
    }

    /// A slot that fixes nothing: the value is what the language says it is,
    /// and a container or class states that type on the wire.
    fn open<V: HostValue>(&self, value: &V, slot: &RuntimeTy) -> Result<Encoded, EncodeError> {
        if value.is_null() {
            return Ok(exact(InboundValue::default(), RuntimeTy::Null));
        }
        if let Some((enumeration, variant)) = value.enum_member() {
            return match self.facts.enumeration(&enumeration) {
                Ok(facts) => self.variant_of(facts, variant, Fit::AsIs),
                // The facts do not reach this enum (no descriptor, or a
                // declaration the program made at run time): the host names
                // what it has and the engine judges it.
                Err(kind) if matches!(*kind, EncodeErrorKind::UnknownEnum(_)) => Ok(Encoded {
                    value: variant_wire(&enumeration, variant),
                    ty: RuntimeTy::Enum(enumeration),
                    fit: Fit::AsIs,
                }),
                Err(kind) => Err(self.err(kind)),
            };
        }
        if let Some(instance) = value.instance() {
            let (wire, args, fit) = match self.facts.class(&instance.class) {
                Ok(class) => {
                    let args = match instance.type_args {
                        Some(args) => Some(args),
                        None if class.generic_params.is_empty() => Some(Vec::new()),
                        // Solved from the fields.
                        None => None,
                    };
                    let (wire, fit, args) = self.fields(
                        &instance.fields,
                        class,
                        args.as_deref(),
                        FieldSource::Instance,
                    )?;
                    (wire, args, fit)
                }
                // The facts do not reach this class: the host names it and
                // its fields are what they are; the engine checks them
                // against the class it knows.
                Err(kind) if matches!(*kind, EncodeErrorKind::UnknownClass(_)) => {
                    let mut fit = Fit::AsIs;
                    let fields = instance
                        .fields
                        .iter()
                        .map(|(name, field)| {
                            let encoded = self
                                .child(PathSegment::Field(name.clone()))
                                .open(field, slot)?;
                            fit = fit.and(encoded.fit);
                            Ok(InboundMapEntry {
                                key: Some(Key::StringKey(name.clone())),
                                value: Some(encoded.value),
                            })
                        })
                        .collect::<Result<Vec<_>, EncodeError>>()?;
                    let wire = InboundValue {
                        value_type: None,
                        value: Some(Value::ClassValue(InboundClassValue { fields })),
                    };
                    (wire, instance.type_args.unwrap_or_default(), fit)
                }
                Err(kind) => return Err(self.err(kind)),
            };
            return Ok(stated(
                wire,
                RuntimeTy::Class(instance.class, args.into()),
                fit,
            ));
        }
        if let Some(items) = value.list() {
            let mut fit = Fit::AsIs;
            let mut element_tys = Vec::new();
            let values = items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let encoded = self.child(PathSegment::Index(index)).open(item, slot)?;
                    fit = fit.and(encoded.fit);
                    push_distinct(&mut element_tys, encoded.ty);
                    Ok(encoded.value)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let element = Self::join(element_tys);
            return Ok(stated(
                InboundValue {
                    value_type: None,
                    value: Some(Value::ListValue(InboundListValue { values })),
                },
                RuntimeTy::List(Box::new(element)),
                fit,
            ));
        }
        if let Some(entries) = value.map() {
            let mut fit = Fit::AsIs;
            let mut key_tys = Vec::new();
            let mut value_tys = Vec::new();
            let encoded_entries = entries
                .iter()
                .map(|(key, item)| {
                    let key_walk = self.child(PathSegment::Key(key.describe()));
                    let encoded_key = key_walk.open(key, slot)?;
                    let wire_key =
                        key_walk.wire_key(encoded_key.value, key, &RuntimeTy::Unknown)?;
                    push_distinct(&mut key_tys, encoded_key.ty);
                    let encoded = self
                        .child(PathSegment::Field(key_label(&wire_key)))
                        .open(item, slot)?;
                    fit = fit.and(encoded.fit);
                    push_distinct(&mut value_tys, encoded.ty);
                    Ok(InboundMapEntry {
                        key: Some(wire_key),
                        value: Some(encoded.value),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let key = if key_tys.is_empty() {
                RuntimeTy::String
            } else {
                Self::join(key_tys)
            };
            let item = Self::join(value_tys);
            return Ok(stated(
                InboundValue {
                    value_type: None,
                    value: Some(Value::MapValue(InboundMapValue {
                        entries: encoded_entries,
                    })),
                },
                RuntimeTy::Map {
                    key: Box::new(key),
                    value: Box::new(item),
                },
                fit,
            ));
        }
        // A callable under an open slot has no signature the host can state;
        // it crosses as a host callable and the engine binds it to the type
        // it declares for the slot.
        match value.callable(slot) {
            Some(Ok(wire)) => return Ok(exact(wire, RuntimeTy::Unknown)),
            Some(Err(error)) => return Err(self.err(EncodeErrorKind::Host(error.0))),
            None => {}
        }
        match value.scalar(ScalarSlot::Open) {
            Answer::AsIs(scalar) | Answer::Converted(scalar) => {
                let ty = scalar_ty(&scalar);
                Ok(exact(scalar_wire(scalar), ty))
            }
            Answer::No => Err(self.err(EncodeErrorKind::NoBamlType {
                found: value.describe(),
            })),
        }
    }

    /// The element type a container has when its slot does not fix one: the
    /// one type its elements share, their union otherwise, and `never` for no
    /// elements, which is the exact type of an empty container.
    fn join(mut tys: Vec<RuntimeTy>) -> RuntimeTy {
        if tys.iter().any(|ty| matches!(ty, RuntimeTy::Unknown)) {
            return RuntimeTy::Unknown;
        }
        match tys.len() {
            0 => RuntimeTy::Never,
            1 => tys.pop().expect("one element"),
            _ => RuntimeTy::Union(tys.into()),
        }
    }

    fn union<V: HostValue>(
        &self,
        value: &V,
        members: &[RuntimeTy],
    ) -> Result<Encoded, EncodeError> {
        let members = self.flatten(members)?;
        let mut as_is = Vec::new();
        let mut converted = Vec::new();
        for member in &members {
            match self.encode(value, member) {
                Ok(encoded) => match encoded.fit {
                    Fit::AsIs => as_is.push(encoded),
                    Fit::Converted => converted.push(encoded),
                },
                Err(error) if error.kind.is_fit_failure() => {}
                Err(error) => return Err(error),
            }
        }
        // A value that is a member of several arms as it is (an empty list
        // under `int[] | string[]`) takes the first arm in declared order.
        // A value that only converts into several arms is a real ambiguity:
        // the host has no single meaning for it, so it is declined.
        let chosen = if !as_is.is_empty() {
            as_is.swap_remove(0)
        } else {
            match converted.len() {
                0 => {
                    return Err(self.err(EncodeErrorKind::NoMember {
                        members: members.iter().map(|m| (*m).clone()).collect(),
                        found: value.describe(),
                    }));
                }
                1 => converted.pop().expect("one candidate"),
                _ => {
                    return Err(self.err(EncodeErrorKind::Ambiguous {
                        members: converted.into_iter().map(|c| c.ty).collect(),
                        found: value.describe(),
                    }));
                }
            }
        };
        Ok(stated(chosen.value, chosen.ty, chosen.fit))
    }

    /// Union members with aliases expanded and nested unions flattened, in
    /// declared order.
    fn flatten<'a>(&'a self, members: &'a [RuntimeTy]) -> Result<Vec<&'a RuntimeTy>, EncodeError> {
        let mut out = Vec::with_capacity(members.len());
        for member in members {
            let member = self.facts.expand(member).map_err(|kind| self.err(kind))?;
            match member {
                RuntimeTy::Union(nested) => out.extend(self.flatten(nested)?),
                other => out.push(other),
            }
        }
        Ok(out)
    }

    fn scalar<V: HostValue>(
        &self,
        value: &V,
        slot: &RuntimeTy,
        kind: ScalarSlot,
    ) -> Result<Encoded, EncodeError> {
        let (scalar, fit) = match value.scalar(kind) {
            Answer::AsIs(scalar) => (scalar, Fit::AsIs),
            Answer::Converted(scalar) => (scalar, Fit::Converted),
            Answer::No => return Err(self.mismatch(value, slot)),
        };
        debug_assert!(
            scalar_ty(&scalar) == *slot,
            "a host answered a {kind:?} slot with {scalar:?}"
        );
        if let Scalar::Int(int) = scalar
            && baml_type::Int63::new(int).is_none()
        {
            return Err(self.err(EncodeErrorKind::NotRepresentable {
                expected: slot.clone(),
                found: value.describe(),
            }));
        }
        Ok(Encoded {
            value: scalar_wire(scalar),
            ty: slot.clone(),
            fit,
        })
    }

    fn literal<V: HostValue>(
        &self,
        value: &V,
        slot: &RuntimeTy,
        literal: &Literal,
    ) -> Result<Encoded, EncodeError> {
        let kind = match literal {
            Literal::Int(_) => ScalarSlot::Int,
            Literal::Bigint(_) => ScalarSlot::Bigint,
            Literal::Float(_) => ScalarSlot::Float,
            Literal::String(_) => ScalarSlot::String,
            Literal::Bool(_) => ScalarSlot::Bool,
        };
        let (scalar, fit) = match value.scalar(kind) {
            Answer::AsIs(scalar) => (scalar, Fit::AsIs),
            Answer::Converted(scalar) => (scalar, Fit::Converted),
            Answer::No => return Err(self.mismatch(value, slot)),
        };
        let equal = match (literal, &scalar) {
            (Literal::Int(expected), Scalar::Int(found)) => expected == found,
            (Literal::Bigint(expected), Scalar::Bigint(found)) => expected == found,
            (Literal::Float(expected), Scalar::Float(found)) => {
                float_literal_admits(expected, *found)
            }
            (Literal::String(expected), Scalar::String(found)) => expected == found,
            (Literal::Bool(expected), Scalar::Bool(found)) => expected == found,
            (
                Literal::Int(_)
                | Literal::Bigint(_)
                | Literal::Float(_)
                | Literal::String(_)
                | Literal::Bool(_),
                _,
            ) => {
                debug_assert!(false, "a host answered a {kind:?} slot with {scalar:?}");
                false
            }
        };
        if !equal {
            return Err(self.err(EncodeErrorKind::LiteralMismatch {
                expected: slot.clone(),
                found: value.describe(),
            }));
        }
        Ok(Encoded {
            value: scalar_wire(scalar),
            ty: slot.clone(),
            fit,
        })
    }

    fn enumeration<V: HostValue>(
        &self,
        value: &V,
        slot: &RuntimeTy,
        name: &TypeName,
        only: Option<&str>,
    ) -> Result<Encoded, EncodeError> {
        let facts = self
            .facts
            .enumeration(name)
            .map_err(|kind| self.err(kind))?;
        let (variant, fit) = match value.enum_variant(facts) {
            Answer::AsIs(variant) => (variant, Fit::AsIs),
            Answer::Converted(variant) => (variant, Fit::Converted),
            Answer::No => return Err(self.mismatch(value, slot)),
        };
        if only.is_some_and(|only| only != variant) {
            return Err(self.err(EncodeErrorKind::LiteralMismatch {
                expected: slot.clone(),
                found: value.describe(),
            }));
        }
        let mut encoded = self.variant_of(facts, variant, fit)?;
        encoded.ty = slot.clone();
        Ok(encoded)
    }

    fn variant_of(
        &self,
        facts: &crate::facts::EnumFacts,
        variant: String,
        fit: Fit,
    ) -> Result<Encoded, EncodeError> {
        if !facts.variants.contains(&variant) {
            return Err(self.err(EncodeErrorKind::UnknownVariant {
                enumeration: facts.name.clone(),
                variant,
            }));
        }
        Ok(Encoded {
            value: variant_wire(&facts.name, variant),
            ty: RuntimeTy::Enum(facts.name.clone()),
            fit,
        })
    }

    fn list<V: HostValue>(
        &self,
        value: &V,
        slot: &RuntimeTy,
        inner: &RuntimeTy,
    ) -> Result<Encoded, EncodeError> {
        let Some(items) = value.list() else {
            return Err(self.mismatch(value, slot));
        };
        let mut fit = Fit::AsIs;
        let mut element_tys = Vec::new();
        let values = items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let encoded = self.child(PathSegment::Index(index)).encode(item, inner)?;
                fit = fit.and(encoded.fit);
                push_distinct(&mut element_tys, encoded.ty);
                Ok(encoded.value)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let wire = InboundValue {
            value_type: None,
            value: Some(Value::ListValue(InboundListValue { values })),
        };
        if mentions_type_var(inner) {
            // The elements determine the element type; the node states it.
            let element = Self::join(element_tys);
            Ok(stated(wire, RuntimeTy::List(Box::new(element)), fit))
        } else {
            Ok(Encoded {
                value: wire,
                ty: slot.clone(),
                fit,
            })
        }
    }

    fn map<V: HostValue>(
        &self,
        value: &V,
        slot: &RuntimeTy,
        key_slot: &RuntimeTy,
        value_slot: &RuntimeTy,
    ) -> Result<Encoded, EncodeError> {
        let Some(entries) = value.map() else {
            return Err(self.mismatch(value, slot));
        };
        let mut fit = Fit::AsIs;
        let mut value_tys = Vec::new();
        let encoded_entries = entries
            .iter()
            .map(|(key, item)| {
                let key_walk = self.child(PathSegment::Key(key.describe()));
                let encoded_key = key_walk.encode(key, key_slot)?;
                fit = fit.and(encoded_key.fit);
                let wire_key = key_walk.wire_key(encoded_key.value, key, key_slot)?;
                let encoded = self
                    .child(PathSegment::Field(key_label(&wire_key)))
                    .encode(item, value_slot)?;
                fit = fit.and(encoded.fit);
                push_distinct(&mut value_tys, encoded.ty);
                Ok(InboundMapEntry {
                    key: Some(wire_key),
                    value: Some(encoded.value),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let wire = InboundValue {
            value_type: None,
            value: Some(Value::MapValue(InboundMapValue {
                entries: encoded_entries,
            })),
        };
        if mentions_type_var(value_slot) || mentions_type_var(key_slot) {
            let item = Self::join(value_tys);
            let key = if mentions_type_var(key_slot) {
                RuntimeTy::String
            } else {
                key_slot.clone()
            };
            Ok(stated(
                wire,
                RuntimeTy::Map {
                    key: Box::new(key),
                    value: Box::new(item),
                },
                fit,
            ))
        } else {
            Ok(Encoded {
                value: wire,
                ty: slot.clone(),
                fit,
            })
        }
    }

    /// An encoded key as the wire's map key: the wire keys maps by string,
    /// int, bool or enum variant.
    fn wire_key<V: HostValue>(
        &self,
        encoded: InboundValue,
        key: &V,
        key_slot: &RuntimeTy,
    ) -> Result<Key, EncodeError> {
        match encoded.value {
            Some(Value::StringValue(string)) => Ok(Key::StringKey(string)),
            Some(Value::IntValue(int)) => Ok(Key::IntKey(int)),
            Some(Value::BoolValue(bool)) => Ok(Key::BoolKey(bool)),
            Some(Value::EnumValue(variant)) => Ok(Key::EnumKey(variant)),
            Some(_) | None => Err(self.err(EncodeErrorKind::MapKey {
                expected: key_slot.clone(),
                found: key.describe(),
            })),
        }
    }

    fn class<V: HostValue>(
        &self,
        value: &V,
        slot: &RuntimeTy,
        name: &TypeName,
        slot_args: &[RuntimeTy],
    ) -> Result<Encoded, EncodeError> {
        let class = self.facts.class(name).map_err(|kind| self.err(kind))?;
        let unrealized = mentions_type_var(slot);
        // The class's type arguments: the slot's when it is realized, the
        // instance's own when it carries them, and otherwise solved from the
        // fields (`None` below).
        let (fields, args, source) = if let Some(instance) = value.instance() {
            if instance.class != *name {
                return Err(self.mismatch(value, slot));
            }
            let args = match instance.type_args {
                Some(args) if !unrealized && args[..] != *slot_args => {
                    return Err(self.mismatch(value, slot));
                }
                Some(args) => Some(args),
                None if unrealized && !class.generic_params.is_empty() => None,
                None => Some(slot_args.to_vec()),
            };
            (instance.fields, args, FieldSource::Instance)
        } else if let Some(entries) = value.map() {
            let args = (!unrealized || class.generic_params.is_empty()).then(|| slot_args.to_vec());
            let fields = entries
                .into_iter()
                .map(|(key, item)| match key.scalar(ScalarSlot::String) {
                    Answer::AsIs(Scalar::String(key)) => Ok((key, item)),
                    Answer::AsIs(_) | Answer::Converted(_) | Answer::No => {
                        Err(self.err(EncodeErrorKind::MapKey {
                            expected: RuntimeTy::String,
                            found: key.describe(),
                        }))
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            (fields, args, FieldSource::Mapping)
        } else {
            return Err(self.mismatch(value, slot));
        };
        let (wire, fit, args) = self.fields(&fields, class, args.as_deref(), source)?;
        // A class node always names its class: the wire's `class_value` has
        // no name of its own, and every typed SDK states it the same way.
        Ok(stated(
            wire,
            RuntimeTy::Class(name.clone(), args.into()),
            fit,
        ))
    }

    /// Encode a class's fields against their declared types. With `args`, the
    /// class's parameters are substituted first. Without, each field is
    /// encoded against its declared type with the parameters open, and the
    /// parameters are solved from the types the fields turn out to have.
    /// Returns the arguments used or solved.
    ///
    /// The solve belongs to the host, not the engine: what `Box { value: "hi" }`
    /// is (`Box<string>`, `Box<"hi">`, …) is a question about the host
    /// language, and a typed host states its arguments outright and never
    /// reaches this. A dynamic host answers with the natural type each field
    /// value has, so the engine receives one definite type and solves nothing.
    fn fields<V: HostValue>(
        &self,
        fields: &[(String, V)],
        class: &crate::facts::ClassFacts,
        args: Option<&[RuntimeTy]>,
        source: FieldSource,
    ) -> Result<(InboundValue, Fit, Vec<RuntimeTy>), EncodeError> {
        let resolve = |param: &ParamTy| {
            let args = args?;
            usize::try_from(param.index())
                .ok()
                .filter(|index| *index < class.generic_params.len())
                .and_then(|index| args.get(index).cloned())
        };
        let mut solved: Vec<Option<RuntimeTy>> = vec![None; class.generic_params.len()];
        let mut fit = match source {
            FieldSource::Instance => Fit::AsIs,
            FieldSource::Mapping => Fit::Converted,
        };
        if let FieldSource::Mapping = source
            && let Some((unknown, _)) = fields
                .iter()
                .find(|(name, _)| !class.fields.iter().any(|(declared, _)| declared == name))
        {
            return Err(self.err(EncodeErrorKind::UnknownField {
                class: class.name.clone(),
                field: unknown.clone(),
            }));
        }
        let mut wire_fields = Vec::with_capacity(class.fields.len());
        for (name, declared) in &class.fields {
            let slot = substitute(declared, &resolve);
            let walk = self.child(PathSegment::Field(name.clone()));
            let encoded = match fields.iter().find(|(field, _)| field == name) {
                Some((_, value)) => walk.encode(value, &slot)?,
                None => {
                    let expanded = walk.facts.expand(&slot).map_err(|kind| walk.err(kind))?;
                    let nullable = matches!(expanded, RuntimeTy::Null)
                        || matches!(expanded, RuntimeTy::Union(members)
                            if members.iter().any(RuntimeTy::is_null));
                    if let (FieldSource::Mapping, true) = (source, nullable) {
                        exact(InboundValue::default(), RuntimeTy::Null)
                    } else {
                        return Err(walk.err(EncodeErrorKind::MissingField {
                            class: class.name.clone(),
                            field: name.clone(),
                        }));
                    }
                }
            };
            fit = fit.and(encoded.fit);
            if args.is_none()
                && let Err(index) = unify(declared, &encoded.ty, &mut solved)
            {
                return Err(walk.err(EncodeErrorKind::TypeArgConflict {
                    class: class.name.clone(),
                    param: class.generic_params[index].as_str().to_string(),
                }));
            }
            wire_fields.push(InboundMapEntry {
                key: Some(Key::StringKey(name.clone())),
                value: Some(encoded.value),
            });
        }
        let args = match args {
            Some(args) => args.to_vec(),
            None => solved
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| self.err(EncodeErrorKind::MissingTypeArgs(class.name.clone())))?,
        };
        Ok((
            InboundValue {
                value_type: None,
                value: Some(Value::ClassValue(InboundClassValue {
                    fields: wire_fields,
                })),
            },
            fit,
            args,
        ))
    }

    /// A host-owned leaf in `slot`. A handle is judged live by the engine and
    /// fits any slot; the others fit the slot of their kind, or a slot that
    /// fixes nothing.
    fn leaf_in<V: HostValue>(
        &self,
        value: &V,
        leaf: Leaf,
        slot: &RuntimeTy,
    ) -> Result<Encoded, EncodeError> {
        let own_ty = match leaf.kind {
            LeafKind::Handle => RuntimeTy::Unknown,
            LeafKind::Media(kind) => RuntimeTy::Media(kind),
            LeafKind::Prompt => RuntimeTy::PromptAst,
            LeafKind::TypeValue => RuntimeTy::Type,
        };
        let fits = |slot: &RuntimeTy| match (leaf.kind, slot) {
            (LeafKind::Handle, _) => true,
            (LeafKind::Media(kind), RuntimeTy::Media(declared)) => {
                *declared == MediaKind::Generic || *declared == kind
            }
            (LeafKind::Media(kind), RuntimeTy::Class(name, args)) => {
                args.is_empty()
                    && MediaKind::from_wrapper_class_name(&name.render_dotted(false))
                        .is_some_and(|wrapper| wrapper == MediaKind::Generic || wrapper == kind)
            }
            (LeafKind::Prompt, RuntimeTy::PromptAst) | (LeafKind::TypeValue, RuntimeTy::Type) => {
                true
            }
            // `ai.Prompt` is the class a rendered prompt inhabits; a program
            // declares it as a class, the wire carries the prompt itself.
            (LeafKind::Prompt, RuntimeTy::Class(name, args)) => {
                args.is_empty() && name.render_dotted(false) == <RuntimeTy>::PromptAst.to_string()
            }
            (LeafKind::Media(_) | LeafKind::Prompt | LeafKind::TypeValue, _) => false,
        };
        let open = matches!(
            slot,
            RuntimeTy::Unknown | RuntimeTy::Interface(..) | RuntimeTy::TypeVar(_)
        );
        let member_fits = match slot {
            RuntimeTy::Union(members) => self.flatten(members)?.into_iter().any(fits),
            other => fits(other),
        };
        if open || member_fits {
            Ok(exact(leaf.value, own_ty))
        } else {
            Err(self.mismatch(value, slot))
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum FieldSource {
    Instance,
    Mapping,
}

fn exact(value: InboundValue, ty: RuntimeTy) -> Encoded {
    Encoded {
        value,
        ty,
        fit: Fit::AsIs,
    }
}

/// A node that states its type: a list, map or class carries the type it has;
/// a scalar, null or enum is self-evident and carries nothing.
fn stated(mut value: InboundValue, ty: RuntimeTy, fit: Fit) -> Encoded {
    let needs_statement = matches!(
        value.value,
        Some(Value::ListValue(_) | Value::MapValue(_) | Value::ClassValue(_))
    );
    if needs_statement && value.value_type.is_none() {
        value.value_type = Some(runtime_ty_to_proto_ty(&ty));
    }
    Encoded { value, ty, fit }
}

/// An enum variant as the wire names it.
fn variant_wire(enumeration: &TypeName, variant: String) -> InboundValue {
    InboundValue {
        value_type: None,
        value: Some(Value::EnumValue(InboundEnumValue {
            name: enumeration.render_dotted(false),
            value: variant,
        })),
    }
}

/// A float literal type has exactly one member: the value its source text
/// denotes. Membership is equality, as the VM compares floats.
#[expect(
    clippy::float_cmp,
    reason = "literal membership is exact equality, not a tolerance"
)]
fn float_literal_admits(source: &str, found: f64) -> bool {
    source
        .parse::<f64>()
        .is_ok_and(|expected| expected == found)
}

/// The key as an error path names it.
fn key_label(key: &Key) -> String {
    match key {
        Key::StringKey(string) => string.clone(),
        Key::IntKey(int) => int.to_string(),
        Key::BoolKey(bool) => bool.to_string(),
        Key::EnumKey(variant) => variant.value.clone(),
    }
}

fn push_distinct(tys: &mut Vec<RuntimeTy>, ty: RuntimeTy) {
    if !tys.contains(&ty) {
        tys.push(ty);
    }
}

fn scalar_ty(scalar: &Scalar) -> RuntimeTy {
    match scalar {
        Scalar::Int(_) => RuntimeTy::Int,
        Scalar::Bigint(_) => RuntimeTy::Bigint,
        Scalar::Float(_) => RuntimeTy::Float,
        Scalar::Bool(_) => RuntimeTy::Bool,
        Scalar::String(_) => RuntimeTy::String,
        Scalar::Bytes(_) => RuntimeTy::Uint8Array,
    }
}

fn scalar_wire(scalar: Scalar) -> InboundValue {
    InboundValue {
        value_type: None,
        value: Some(match scalar {
            Scalar::Int(int) => Value::IntValue(int),
            Scalar::Bigint(bigint) => Value::BigintValue(hex(&bigint)),
            Scalar::Float(float) => Value::FloatValue(float),
            Scalar::Bool(bool) => Value::BoolValue(bool),
            Scalar::String(string) => Value::StringValue(string),
            Scalar::Bytes(bytes) => Value::Uint8arrayValue(bytes),
        }),
    }
}

/// Bigints cross the wire in base sixteen with a leading minus.
fn hex(value: &BigInt) -> String {
    format!("{value:x}")
}

impl EncodeErrorKind {
    /// The value simply is not a member of the slot's type. A union keeps
    /// trying its other members; any other kind is a fault in the facts or
    /// the host and ends the walk.
    fn is_fit_failure(&self) -> bool {
        match self {
            Self::Mismatch { .. }
            | Self::NoMember { .. }
            | Self::Ambiguous { .. }
            | Self::UnknownField { .. }
            | Self::MissingField { .. }
            | Self::UnknownVariant { .. }
            | Self::LiteralMismatch { .. }
            | Self::NotRepresentable { .. }
            | Self::MapKey { .. }
            | Self::MissingTypeArgs(_)
            | Self::TypeArgConflict { .. }
            | Self::NoBamlType { .. } => true,
            Self::UnknownClass(_)
            | Self::UnknownEnum(_)
            | Self::UnknownAlias(_)
            | Self::AliasCycle(_)
            | Self::Host(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use baml_sdkgen_types::{
        ClassDescriptor, EnumDescriptor, FieldDescriptor, FunctionDescriptor, ParamDescriptor,
        ProgramDescriptor, Ty, TypeAliasDescriptor,
    };
    use baml_type::{Freshness, FunctionParamMode, Int63};
    use bridge_ctypes::baml_bridge::cffi::{BamlHandle, BamlValueMedia, baml_ty::Ty as WireTy};

    use super::*;
    use crate::host::{HostError, Instance, Leaf, LeafKind};

    /// A host value with Python's rules: `bool` is not `int`, an `int` fits
    /// a `float` or `bigint` slot by conversion, a string names a variant.
    #[derive(Debug, Clone)]
    enum Mock {
        Null,
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(&'static str),
        List(Vec<Mock>),
        Map(Vec<(Mock, Mock)>),
        Instance {
            class: &'static str,
            args: Option<Vec<RuntimeTy>>,
            fields: Vec<(&'static str, Mock)>,
        },
        Member(&'static str, &'static str),
        Callable,
        Handle,
        Media(MediaKind),
    }

    fn tn(name: &str) -> TypeName {
        TypeName::new(Name::new("user"), Vec::new(), Name::new(name))
    }

    impl HostValue for Mock {
        fn is_null(&self) -> bool {
            matches!(self, Mock::Null)
        }

        fn scalar(&self, slot: ScalarSlot) -> Answer<Scalar> {
            match (self, slot) {
                (Mock::Int(i), ScalarSlot::Int | ScalarSlot::Open) => Answer::AsIs(Scalar::Int(*i)),
                (Mock::Int(i), ScalarSlot::Float) => match exact_float(*i) {
                    Some(float) => Answer::Converted(Scalar::Float(float)),
                    None => Answer::No,
                },
                (Mock::Int(i), ScalarSlot::Bigint) => {
                    Answer::Converted(Scalar::Bigint(BigInt::from(*i)))
                }
                (Mock::Float(f), ScalarSlot::Float | ScalarSlot::Open) => {
                    Answer::AsIs(Scalar::Float(*f))
                }
                (Mock::Bool(b), ScalarSlot::Bool | ScalarSlot::Open) => {
                    Answer::AsIs(Scalar::Bool(*b))
                }
                (Mock::Str(s), ScalarSlot::String | ScalarSlot::Open) => {
                    Answer::AsIs(Scalar::String((*s).to_string()))
                }
                _ => Answer::No,
            }
        }

        fn enum_member(&self) -> Option<(TypeName, String)> {
            match self {
                Mock::Member(enumeration, variant) => {
                    Some((tn(enumeration), (*variant).to_string()))
                }
                _ => None,
            }
        }

        fn enum_variant(&self, enumeration: &crate::facts::EnumFacts) -> Answer<String> {
            match self {
                Mock::Str(s) => Answer::Converted((*s).to_string()),
                _ => match self.enum_member() {
                    Some((name, variant)) if name == enumeration.name => Answer::AsIs(variant),
                    _ => Answer::No,
                },
            }
        }

        fn list(&self) -> Option<Vec<Self>> {
            match self {
                Mock::List(items) => Some(items.clone()),
                _ => None,
            }
        }

        fn map(&self) -> Option<Vec<(Self, Self)>> {
            match self {
                Mock::Map(entries) => Some(entries.clone()),
                _ => None,
            }
        }

        fn instance(&self) -> Option<Instance<Self>> {
            match self {
                Mock::Instance {
                    class,
                    args,
                    fields,
                } => Some(Instance {
                    class: tn(class),
                    type_args: args.clone(),
                    fields: fields
                        .iter()
                        .map(|(name, value)| ((*name).to_string(), value.clone()))
                        .collect(),
                }),
                _ => None,
            }
        }

        fn callable(&self, _signature: &RuntimeTy) -> Option<Result<InboundValue, HostError>> {
            matches!(self, Mock::Callable).then(|| Ok(handle_wire(7)))
        }

        fn leaf(&self) -> Option<Result<Leaf, HostError>> {
            match self {
                Mock::Handle => Some(Ok(Leaf {
                    value: handle_wire(9),
                    kind: LeafKind::Handle,
                })),
                Mock::Media(kind) => Some(Ok(Leaf {
                    value: InboundValue {
                        value_type: None,
                        value: Some(Value::MediaValue(BamlValueMedia::default())),
                    },
                    kind: LeafKind::Media(*kind),
                })),
                _ => None,
            }
        }

        fn describe(&self) -> String {
            format!("{self:?}")
        }
    }

    fn handle_wire(key: u64) -> InboundValue {
        InboundValue {
            value_type: None,
            value: Some(Value::Handle(BamlHandle {
                key,
                ..Default::default()
            })),
        }
    }

    fn class(name: &str, args: Vec<Ty>) -> Ty {
        Ty::Class(tn(name), args.into())
    }

    fn var(index: u32, name: &str) -> Ty {
        Ty::TypeVar(ParamTy::new(index, Name::new(name)))
    }

    fn union(members: Vec<Ty>) -> Ty {
        Ty::Union(members.into())
    }

    fn param(name: &str, ty: Ty) -> ParamDescriptor {
        ParamDescriptor {
            name: Name::new(name),
            ty,
            has_default: false,
        }
    }

    fn field(name: &str, ty: Ty) -> FieldDescriptor {
        FieldDescriptor {
            name: Name::new(name),
            ty,
        }
    }

    fn descriptor() -> ProgramDescriptor {
        ProgramDescriptor {
            functions: vec![FunctionDescriptor {
                fqn: "user.f".into(),
                class_generic_params: vec![],
                generic_params: vec![Name::new("T")],
                params: vec![
                    param("a", Ty::Int),
                    param("b", Ty::Float),
                    param("c", Ty::Enum(tn("Mood"))),
                    param("d", class("Box", vec![var(0, "T")])),
                    param("e", union(vec![Ty::Int, Ty::String])),
                    param("g", Ty::Unknown),
                    param(
                        "h",
                        Ty::Function {
                            params: Box::new([baml_type::CodegenFunctionParamTy {
                                name: None,
                                ty: Ty::Int,
                                mode: FunctionParamMode::Required,
                            }]),
                            ret: Box::new(Ty::String),
                            throws: Box::new(Ty::Never),
                        },
                    ),
                    param(
                        "i",
                        union(vec![
                            Ty::Literal(Literal::Int(1), Freshness::Regular),
                            Ty::Literal(Literal::Int(2), Freshness::Regular),
                        ]),
                    ),
                    param(
                        "j",
                        Ty::Map {
                            key: Box::new(Ty::String),
                            value: Box::new(Ty::Int),
                        },
                    ),
                    param(
                        "k",
                        union(vec![
                            Ty::List(Box::new(Ty::Int)),
                            Ty::List(Box::new(Ty::String)),
                        ]),
                    ),
                    param("l", union(vec![class("A", vec![]), class("B", vec![])])),
                    param("m", Ty::TypeAlias(tn("Tree"))),
                    param("n", union(vec![Ty::Int, Ty::Float])),
                    param("o", union(vec![Ty::Enum(tn("Mood")), Ty::String])),
                    param("p", Ty::List(Box::new(var(0, "T")))),
                    param("q", Ty::Media(MediaKind::Image)),
                    param("r", union(vec![Ty::Media(MediaKind::Pdf), Ty::Null])),
                ],
                ret: Ty::Void,
                throws: Ty::Never,
            }],
            classes: vec![
                ClassDescriptor {
                    name: tn("Box"),
                    generic_params: vec![Name::new("T")],
                    fields: vec![field("item", var(0, "T"))],
                },
                ClassDescriptor {
                    name: tn("A"),
                    generic_params: vec![],
                    fields: vec![field("x", Ty::Int)],
                },
                ClassDescriptor {
                    name: tn("B"),
                    generic_params: vec![],
                    fields: vec![
                        field("x", Ty::Int),
                        field("y", union(vec![Ty::Int, Ty::Null])),
                    ],
                },
            ],
            enums: vec![EnumDescriptor {
                name: tn("Mood"),
                variants: vec![Name::new("Happy"), Name::new("Sad")],
            }],
            type_aliases: vec![TypeAliasDescriptor {
                name: tn("Tree"),
                resolves_to: union(vec![Ty::Int, Ty::List(Box::new(Ty::TypeAlias(tn("Tree"))))]),
                recursive: true,
            }],
        }
    }

    struct Fixture {
        facts: Facts,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                facts: Facts::from_descriptor(&descriptor()),
            }
        }

        fn slot(&self, name: &str) -> RuntimeTy {
            self.facts
                .function("user.f")
                .unwrap()
                .params
                .iter()
                .find(|p| p.name == name)
                .unwrap()
                .ty
                .clone()
        }

        fn encode(&self, value: &Mock, param: &str) -> Result<Encoded, EncodeError> {
            let slot = self.slot(param);
            encode(
                &self.facts,
                value,
                &slot,
                vec![PathSegment::Argument(param.to_string())],
            )
        }

        fn ok(&self, value: &Mock, param: &str) -> Encoded {
            self.encode(value, param).unwrap()
        }

        fn kind(&self, value: &Mock, param: &str) -> EncodeErrorKind {
            *self.encode(value, param).unwrap_err().kind
        }
    }

    /// Python's rule for an `int` in a `float` slot: only when the float is
    /// exactly that integer.
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "the round trip checks the cast was exact"
    )]
    fn exact_float(int: i64) -> Option<f64> {
        let float = int as f64;
        (float as i64 == int).then_some(float)
    }

    fn stated_as(encoded: &Encoded) -> Option<RuntimeTy> {
        encoded
            .value
            .value_type
            .as_ref()
            .map(|ty| bridge_ctypes::proto_ty_to_runtime_ty(ty).unwrap())
    }

    fn inst(class: &'static str, fields: Vec<(&'static str, Mock)>) -> Mock {
        Mock::Instance {
            class,
            args: None,
            fields,
        }
    }

    #[test]
    fn scalars_follow_the_slot_and_the_hosts_rules() {
        let f = Fixture::new();
        let a = f.ok(&Mock::Int(3), "a");
        assert_eq!(a.value.value, Some(Value::IntValue(3)));
        assert_eq!((a.fit, a.ty), (Fit::AsIs, RuntimeTy::Int));
        assert!(a.value.value_type.is_none(), "a closed slot states nothing");

        let b = f.ok(&Mock::Int(3), "b");
        assert_eq!(b.value.value, Some(Value::FloatValue(3.0)));
        assert_eq!((b.fit, b.ty), (Fit::Converted, RuntimeTy::Float));

        assert!(matches!(
            f.kind(&Mock::Float(1.5), "a"),
            EncodeErrorKind::Mismatch {
                expected: RuntimeTy::Int,
                ..
            }
        ));
        assert!(matches!(
            f.kind(&Mock::Int(Int63::MAX.get() + 1), "a"),
            EncodeErrorKind::NotRepresentable { .. }
        ));
        assert!(matches!(
            f.kind(&Mock::Null, "a"),
            EncodeErrorKind::Mismatch { .. }
        ));
    }

    #[test]
    fn enums_take_members_as_is_and_strings_by_conversion() {
        let f = Fixture::new();
        let member = f.ok(&Mock::Member("Mood", "Sad"), "c");
        assert_eq!(member.fit, Fit::AsIs);
        assert_eq!(
            member.value.value,
            Some(Value::EnumValue(InboundEnumValue {
                name: "user.Mood".into(),
                value: "Sad".into(),
            }))
        );
        assert_eq!(f.ok(&Mock::Str("Happy"), "c").fit, Fit::Converted);
        assert!(matches!(
            f.kind(&Mock::Str("Angry"), "c"),
            EncodeErrorKind::UnknownVariant { variant, .. } if variant == "Angry"
        ));
        assert!(matches!(
            f.kind(&Mock::Member("Other", "Sad"), "c"),
            EncodeErrorKind::Mismatch { .. }
        ));
    }

    #[test]
    fn unions_prefer_as_is_over_converted_and_state_only_containers() {
        let f = Fixture::new();
        let e = f.ok(&Mock::Int(1), "e");
        assert_eq!(e.ty, RuntimeTy::Int);
        assert!(e.value.value_type.is_none(), "a scalar is self-evident");

        // `int | float` with an int: the as-is member wins over the conversion.
        assert_eq!(f.ok(&Mock::Int(1), "n").ty, RuntimeTy::Int);
        // `Mood | string` with a string that names a variant stays a string.
        assert_eq!(f.ok(&Mock::Str("Happy"), "o").ty, RuntimeTy::String);
        assert_eq!(
            f.ok(&Mock::Member("Mood", "Happy"), "o").ty,
            RuntimeTy::Enum(tn("Mood"))
        );

        // `int[] | string[]`: an empty list fits both as is and takes the
        // first declared member; a populated one is decided by its elements.
        let empty = f.ok(&Mock::List(vec![]), "k");
        assert_eq!(stated_as(&empty), Some(RuntimeTy::list(RuntimeTy::Int)));
        let strings = f.ok(&Mock::List(vec![Mock::Str("a")]), "k");
        assert_eq!(
            stated_as(&strings),
            Some(RuntimeTy::list(RuntimeTy::String))
        );

        assert!(matches!(
            f.kind(&Mock::Bool(true), "e"),
            EncodeErrorKind::NoMember { members, .. } if members.len() == 2
        ));
    }

    #[test]
    fn classes_take_instances_as_is_and_mappings_strictly() {
        let f = Fixture::new();
        let instance = f.ok(&inst("A", vec![("x", Mock::Int(1))]), "l");
        assert_eq!(
            (instance.fit, &instance.ty),
            (Fit::AsIs, &RuntimeTy::Class(tn("A"), Box::new([])))
        );
        assert_eq!(
            stated_as(&instance),
            Some(RuntimeTy::Class(tn("A"), Box::new([])))
        );
        assert!(matches!(
            instance.value.value,
            Some(Value::ClassValue(InboundClassValue { fields })) if fields.len() == 1
        ));

        // A mapping converts into B (A fits too): two conversions are ambiguous.
        let shape = Mock::Map(vec![(Mock::Str("x"), Mock::Int(1))]);
        assert!(matches!(
            f.kind(&shape, "l"),
            EncodeErrorKind::Ambiguous { .. }
        ));

        // Against one class a mapping converts; keys are declared names only.
        let slot = RuntimeTy::Class(tn("B"), Box::new([]));
        let shape = Mock::Map(vec![(Mock::Str("x"), Mock::Int(1))]);
        let b = encode(&f.facts, &shape, &slot, vec![]).unwrap();
        assert_eq!(b.fit, Fit::Converted);
        let Some(Value::ClassValue(InboundClassValue { fields })) = &b.value.value else {
            panic!("class value");
        };
        assert_eq!(fields.len(), 2, "the absent nullable field is sent as null");
        assert_eq!(fields[1].value.as_ref().unwrap().value, None);

        let extra = Mock::Map(vec![
            (Mock::Str("x"), Mock::Int(1)),
            (Mock::Str("z"), Mock::Int(1)),
        ]);
        assert!(matches!(
            *encode(&f.facts, &extra, &slot, vec![]).unwrap_err().kind,
            EncodeErrorKind::UnknownField { field, .. } if field == "z"
        ));
        let missing = Mock::Map(vec![(Mock::Str("y"), Mock::Int(1))]);
        assert!(matches!(
            *encode(&f.facts, &missing, &slot, vec![]).unwrap_err().kind,
            EncodeErrorKind::MissingField { field, .. } if field == "x"
        ));
        let wrong_class = inst("A", vec![("x", Mock::Int(1))]);
        assert!(matches!(
            *encode(&f.facts, &wrong_class, &slot, vec![])
                .unwrap_err()
                .kind,
            EncodeErrorKind::Mismatch { .. }
        ));
    }

    #[test]
    fn generic_class_slots_need_the_instances_arguments_unless_bound() {
        let f = Fixture::new();
        let boxed = Mock::Instance {
            class: "Box",
            args: Some(vec![RuntimeTy::Int]),
            fields: vec![("item", Mock::Int(1))],
        };
        let d = f.ok(&boxed, "d");
        assert_eq!(
            stated_as(&d),
            Some(RuntimeTy::Class(tn("Box"), Box::new([RuntimeTy::Int])))
        );

        // An instance that carries no arguments has them solved from its
        // fields: `item: T` holding an `int` makes it a `Box<int>`.
        let solved = f.ok(&inst("Box", vec![("item", Mock::Int(1))]), "d");
        assert_eq!(
            stated_as(&solved),
            Some(RuntimeTy::Class(tn("Box"), Box::new([RuntimeTy::Int])))
        );
        // A field holding null makes it a `Box<null>`: null is a type too.
        assert_eq!(
            stated_as(&f.ok(&inst("Box", vec![("item", Mock::Null)]), "d")),
            Some(RuntimeTy::Class(tn("Box"), Box::new([RuntimeTy::Null])))
        );

        // Bound explicitly, the slot is realized and the instance needs no args.
        let signature = f.facts.function("user.f").unwrap();
        let bindings = HashMap::from([(Name::new("T"), RuntimeTy::Float)]);
        let entries = encode_arguments(
            &f.facts,
            signature,
            &bindings,
            &[("d".to_string(), inst("Box", vec![("item", Mock::Int(1))]))],
        )
        .unwrap();
        let value = entries[0].value.as_ref().unwrap();
        assert!(matches!(
            value.value_type.as_ref().and_then(|ty| ty.ty.as_ref()),
            Some(WireTy::ClassTy(class)) if class.type_args.len() == 1
        ));
        let Some(Value::ClassValue(InboundClassValue { fields })) = &value.value else {
            panic!("class value");
        };
        assert_eq!(
            fields[0].value.as_ref().unwrap().value,
            Some(Value::FloatValue(1.0))
        );

        // A list over the unbound variable states the element type its
        // elements have; empty, its exact type is `never[]`.
        let p = f.ok(&Mock::List(vec![Mock::Int(1), Mock::Int(2)]), "p");
        assert_eq!(stated_as(&p), Some(RuntimeTy::list(RuntimeTy::Int)));
        assert_eq!(
            stated_as(&f.ok(&Mock::List(vec![]), "p")),
            Some(RuntimeTy::list(RuntimeTy::Never))
        );
    }

    #[test]
    fn open_slots_take_the_values_own_type() {
        let f = Fixture::new();
        assert_eq!(f.ok(&Mock::Int(1), "g").ty, RuntimeTy::Int);
        assert!(f.ok(&Mock::Int(1), "g").value.value_type.is_none());
        assert_eq!(f.ok(&Mock::Null, "g").ty, RuntimeTy::Null);
        assert_eq!(
            f.ok(&Mock::Member("Mood", "Sad"), "g").ty,
            RuntimeTy::Enum(tn("Mood"))
        );

        let mixed = f.ok(
            &Mock::List(vec![Mock::Int(1), Mock::Str("a"), Mock::Int(2)]),
            "g",
        );
        assert_eq!(
            stated_as(&mixed),
            Some(RuntimeTy::list(RuntimeTy::Union(Box::new([
                RuntimeTy::Int,
                RuntimeTy::String
            ]))))
        );
        assert_eq!(
            stated_as(&f.ok(&Mock::List(vec![]), "g")),
            Some(RuntimeTy::list(RuntimeTy::Never))
        );

        let map = f.ok(&Mock::Map(vec![(Mock::Str("k"), Mock::Float(1.5))]), "g");
        assert_eq!(
            stated_as(&map),
            Some(RuntimeTy::map(RuntimeTy::String, RuntimeTy::Float))
        );

        let a = f.ok(&inst("A", vec![("x", Mock::Int(1))]), "g");
        assert_eq!(stated_as(&a), Some(RuntimeTy::Class(tn("A"), Box::new([]))));
        assert_eq!(
            stated_as(&f.ok(&inst("Box", vec![("item", Mock::Str("s"))]), "g")),
            Some(RuntimeTy::Class(tn("Box"), Box::new([RuntimeTy::String])))
        );
        assert!(matches!(
            f.kind(&Mock::Callable, "a"),
            EncodeErrorKind::Mismatch { .. }
        ));
        // A callable under an open slot crosses as a host callable the engine
        // binds; the host states no type for it.
        assert_eq!(f.ok(&Mock::Callable, "g").ty, RuntimeTy::Unknown);
    }

    #[test]
    fn without_facts_the_host_names_what_it_has() {
        // No descriptor at all: the walker has no classes or enums, so an
        // instance names its class and its fields go as they are, and an
        // enum member names its enum.
        let facts = Facts::default();
        let instance = inst("A", vec![("x", Mock::Int(1)), ("s", Mock::Str("k"))]);
        let encoded = encode(&facts, &instance, &RuntimeTy::Unknown, vec![]).unwrap();
        assert_eq!(
            stated_as(&encoded),
            Some(RuntimeTy::Class(tn("A"), Box::new([])))
        );
        let Some(Value::ClassValue(InboundClassValue { fields })) = &encoded.value.value else {
            panic!("class value");
        };
        assert_eq!(fields.len(), 2);
        let member = encode(
            &facts,
            &Mock::Member("Mood", "Sad"),
            &RuntimeTy::Unknown,
            vec![],
        )
        .unwrap();
        assert_eq!(member.ty, RuntimeTy::Enum(tn("Mood")));
        // A declared slot still needs the facts.
        assert!(matches!(
            *encode(
                &facts,
                &instance,
                &RuntimeTy::Class(tn("A"), Box::new([])),
                vec![]
            )
            .unwrap_err()
            .kind,
            EncodeErrorKind::UnknownClass(_)
        ));
    }

    #[test]
    fn literals_maps_callables_and_aliases() {
        let f = Fixture::new();
        assert_eq!(
            f.ok(&Mock::Int(2), "i").value.value,
            Some(Value::IntValue(2))
        );
        assert!(matches!(
            f.kind(&Mock::Int(3), "i"),
            EncodeErrorKind::NoMember { .. }
        ));

        let j = f.ok(&Mock::Map(vec![(Mock::Str("k"), Mock::Int(1))]), "j");
        assert!(j.value.value_type.is_none());
        assert!(matches!(
            f.kind(&Mock::Map(vec![(Mock::Int(1), Mock::Int(1))]), "j"),
            EncodeErrorKind::Mismatch {
                expected: RuntimeTy::String,
                ..
            }
        ));

        assert_eq!(f.ok(&Mock::Callable, "h").ty, f.slot("h"));
        assert!(matches!(
            f.kind(&Mock::Callable, "a"),
            EncodeErrorKind::Mismatch { .. }
        ));

        // `type Tree = int | Tree[]`: nested lists under the recursive alias.
        let tree = f.ok(
            &Mock::List(vec![Mock::Int(1), Mock::List(vec![Mock::Int(2)])]),
            "m",
        );
        assert_eq!(
            stated_as(&tree),
            Some(RuntimeTy::list(RuntimeTy::TypeAlias(tn("Tree"))))
        );
    }

    #[test]
    fn leaves_fit_their_kind_and_handles_fit_anything() {
        let f = Fixture::new();
        assert_eq!(f.ok(&Mock::Handle, "a").ty, RuntimeTy::Unknown);
        assert_eq!(f.ok(&Mock::Handle, "l").ty, RuntimeTy::Unknown);
        assert_eq!(
            f.ok(&Mock::Media(MediaKind::Image), "q").ty,
            RuntimeTy::Media(MediaKind::Image)
        );
        assert!(matches!(
            f.kind(&Mock::Media(MediaKind::Pdf), "q"),
            EncodeErrorKind::Mismatch { .. }
        ));
        assert_eq!(
            f.ok(&Mock::Media(MediaKind::Pdf), "r").ty,
            RuntimeTy::Media(MediaKind::Pdf)
        );
        assert!(matches!(
            f.kind(&Mock::Media(MediaKind::Image), "a"),
            EncodeErrorKind::Mismatch { .. }
        ));
        assert_eq!(
            f.ok(&Mock::Media(MediaKind::Image), "g").ty,
            RuntimeTy::Media(MediaKind::Image)
        );
    }

    #[test]
    fn arguments_are_keyed_by_declared_name_and_errors_carry_a_path() {
        let f = Fixture::new();
        let signature = f.facts.function("user.f").unwrap();
        let entries = encode_arguments(
            &f.facts,
            signature,
            &HashMap::new(),
            &[
                ("a".to_string(), Mock::Int(1)),
                ("b".to_string(), Mock::Int(2)),
            ],
        )
        .unwrap();
        assert_eq!(entries[0].key, Some(Key::StringKey("a".into())));
        assert_eq!(
            entries[1].value.as_ref().unwrap().value,
            Some(Value::FloatValue(2.0))
        );

        // A name no parameter declares is the engine's to reject; it goes as
        // the type its value has.
        let entries = encode_arguments(
            &f.facts,
            signature,
            &HashMap::new(),
            &[("zz".to_string(), Mock::Int(1))],
        )
        .unwrap();
        assert_eq!(
            entries[0].value.as_ref().unwrap().value,
            Some(Value::IntValue(1))
        );

        let nested = Mock::Map(vec![(Mock::Str("k"), Mock::Str("not an int"))]);
        let error = f.encode(&nested, "j").unwrap_err();
        assert_eq!(
            error.to_string(),
            "argument `j`.k: expected int, found Str(\"not an int\")"
        );
        let error = f
            .encode(&Mock::List(vec![Mock::Int(1), Mock::Float(0.5)]), "k")
            .unwrap_err();
        assert!(matches!(*error.kind, EncodeErrorKind::NoMember { .. }));
    }

    #[test]
    fn stated_types_are_the_wire_types_the_engine_reads() {
        let f = Fixture::new();
        let a = f.ok(&inst("A", vec![("x", Mock::Int(1))]), "g");
        assert!(matches!(
            a.value.value_type.as_ref().and_then(|ty| ty.ty.as_ref()),
            Some(WireTy::ClassTy(class)) if class.name == "user.A"
        ));
    }
}
