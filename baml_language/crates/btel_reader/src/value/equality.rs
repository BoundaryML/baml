//! Bounded semantic equality, independent of CAS IDs, object IDs and rendering.
//! Build borrowed trees so sharing and the blobs a value is stored in are
//! immaterial; cycles and opaque values are explicitly unavailable. Validate
//! both operands before deciding inequality.
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};

use btel_snapshot::{
    CasId, DecodedName, DecodedObject, DecodedRoot, DecodedSnapshot, DecodedValue, NodeId,
};
use serde_json::Value as Json;

use super::{BlobSource, CmpOp, Leaf, Nav, Unavailable, compare, leaf_of, span::Span};
use crate::evidence::ArgumentNames;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_depth: usize,
    pub max_nodes: usize,
    /// Text, keys and bytes examined across both operands, including sharing.
    pub max_bytes: usize,
    /// Blobs read for one comparison beyond the ones its operands start in.
    pub max_blobs: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_depth: 64,
            max_nodes: 100_000,
            max_bytes: 16 << 20,
            max_blobs: 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Evidence(Unavailable),
    Cycle,
    Unsupported,
    Limit,
}

impl Error {
    pub fn code(self) -> &'static str {
        match self {
            Self::Evidence(reason) => reason.code(),
            Self::Cycle => "comparison_cycle",
            Self::Unsupported => "comparison_unsupported",
            Self::Limit => "comparison_limit",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Captured<'a> {
    pub nav: &'a Nav,
    pub names: Option<&'a ArgumentNames>,
}

/// What a comparison starts from, with the values to read the blobs of.
enum Start<'a> {
    Missing,
    Unavailable(Unavailable),
    Value(&'a Arc<DecodedSnapshot>, DecodedValue),
    Arguments(&'a Arc<DecodedSnapshot>),
}

impl<'a> Start<'a> {
    fn of(nav: &'a Nav) -> Self {
        match nav {
            Nav::Missing => Self::Missing,
            Nav::Unavailable(reason) => Self::Unavailable(*reason),
            Nav::Value(found) => Self::Value(found.blob(), found.value().into()),
            Nav::Arguments(snapshot) => Self::Arguments(snapshot),
        }
    }

    fn values(&self) -> Vec<(&Arc<DecodedSnapshot>, &DecodedValue)> {
        match self {
            Self::Missing | Self::Unavailable(_) => Vec::new(),
            Self::Value(blob, value) => vec![(*blob, value)],
            Self::Arguments(snapshot) => match &snapshot.root {
                DecodedRoot::FunctionArgs { slots, .. } => {
                    slots.iter().map(|slot| (*snapshot, slot)).collect()
                }
                DecodedRoot::Value(_) => Vec::new(),
            },
        }
    }
}

type Fields<'a> = BTreeMap<&'a str, Value<'a>>;

enum Value<'a> {
    Scalar(Leaf<'a>),
    Unsigned(u64),
    Omitted,
    Uint8Array(&'a [u8]),
    Enum(&'a DecodedName, &'a str),
    List(Vec<Self>),
    Map(Fields<'a>),
    Class(&'a DecodedName, Fields<'a>),
}

impl Value<'_> {
    fn equal(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Unsigned(a), Self::Unsigned(b)) => a == b,
            (Self::Unsigned(n), Self::Scalar(b)) | (Self::Scalar(b), Self::Unsigned(n)) => {
                compare(Leaf::Bigint(&(*n).into()), CmpOp::Eq, *b) == Some(true)
            }
            (Self::Scalar(a), Self::Scalar(b)) => compare(*a, CmpOp::Eq, *b) == Some(true),
            (Self::Omitted, Self::Omitted) => true,
            (Self::Uint8Array(a), Self::Uint8Array(b)) => a == b,
            (Self::Enum(a, x), Self::Enum(b, y)) => a == b && x == y,
            (Self::List(a), Self::List(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.equal(y))
            }
            (Self::Map(a), Self::Map(b)) => fields_equal(a, b),
            (Self::Class(a, x), Self::Class(b, y)) => a == b && fields_equal(x, y),
            (
                Self::Unsigned(_)
                | Self::Scalar(_)
                | Self::Omitted
                | Self::Uint8Array(_)
                | Self::Enum(..)
                | Self::List(_)
                | Self::Map(_)
                | Self::Class(..),
                _,
            ) => false,
        }
    }
}

fn fields_equal(a: &Fields<'_>, b: &Fields<'_>) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|((k, x), (l, y))| k == l && x.equal(y))
}

struct Builder<'a> {
    limits: &'a Limits,
    span: &'a Span,
    nodes: usize,
    bytes: usize,
    /// Objects on the path being built, each in its blob.
    active: HashSet<(CasId, NodeId)>,
}

impl<'a> Builder<'a> {
    fn new(limits: &'a Limits, span: &'a Span) -> Self {
        Self {
            limits,
            span,
            nodes: 0,
            bytes: 0,
            active: HashSet::new(),
        }
    }

    fn node(&mut self, depth: usize) -> Result<(), Error> {
        // A hard depth ceiling also protects recursive construction and Drop
        // when a caller raises the configurable limits.
        if depth > self.limits.max_depth.min(128) || self.nodes >= self.limits.max_nodes {
            return Err(Error::Limit);
        }
        self.nodes += 1;
        Ok(())
    }

    fn bytes(&mut self, n: usize) -> Result<(), Error> {
        self.bytes = self.bytes.checked_add(n).ok_or(Error::Limit)?;
        if self.bytes > self.limits.max_bytes {
            return Err(Error::Limit);
        }
        Ok(())
    }

    fn captured(
        &mut self,
        start: &'a Start<'a>,
        names: Option<&'a ArgumentNames>,
    ) -> Result<Option<Value<'a>>, Error> {
        match start {
            Start::Missing => Ok(None),
            Start::Unavailable(reason) => Err(Error::Evidence(*reason)),
            Start::Value(blob, value) => self.value(blob, value, 0).map(Some),
            Start::Arguments(snapshot) => {
                let DecodedRoot::FunctionArgs {
                    parameter_count,
                    slots,
                } = &snapshot.root
                else {
                    return Err(Error::Evidence(Unavailable::WrongRoot));
                };
                complete(slots.len(), *parameter_count)?;
                let names = names.ok_or(Error::Evidence(Unavailable::ArgumentNamesUnknown))?;
                if names.slots.len() != slots.len() {
                    return Err(Error::Evidence(Unavailable::ArgumentLayoutMismatch));
                }
                self.node(0)?;
                let mut fields = BTreeMap::new();
                for (slot, v) in names.slots.iter().zip(slots) {
                    let name = slot
                        .name
                        .as_deref()
                        .ok_or(Error::Evidence(Unavailable::ArgumentNamesUnknown))?;
                    self.bytes(name.len())?;
                    if fields.insert(name, self.value(snapshot, v, 1)?).is_some() {
                        return Err(Error::Evidence(Unavailable::ArgumentLayoutMismatch));
                    }
                }
                Ok(Some(Value::Map(fields)))
            }
        }
    }

    fn value(
        &mut self,
        s: &'a DecodedSnapshot,
        v: &'a DecodedValue,
        depth: usize,
    ) -> Result<Value<'a>, Error> {
        self.node(depth)?;
        match v {
            DecodedValue::External(child) => {
                let (child, root) = self.span.root_of(s, *child).map_err(Error::Evidence)?;
                self.stored(child, root, depth)
            }
            DecodedValue::ExternalNode { child, node } => {
                let (child, id) = self
                    .span
                    .node_of(s, *child, *node)
                    .map_err(Error::Evidence)?;
                self.reference(child, id, depth)
            }
            DecodedValue::Null
            | DecodedValue::OmittedArg
            | DecodedValue::Bool(_)
            | DecodedValue::Int(_)
            | DecodedValue::Float(_)
            | DecodedValue::String(_)
            | DecodedValue::Bigint(_)
            | DecodedValue::Object(_)
            | DecodedValue::Type(_)
            | DecodedValue::Enum { .. }
            | DecodedValue::Truncated(_) => self.stored(s, v, depth),
        }
    }

    fn reference(
        &mut self,
        s: &'a DecodedSnapshot,
        id: NodeId,
        depth: usize,
    ) -> Result<Value<'a>, Error> {
        let key = (s.id, id);
        if !self.active.insert(key) {
            return Err(Error::Cycle);
        }
        let result = self.object(s, s.object(id), depth);
        self.active.remove(&key);
        result
    }

    /// A value in the blob that stores it.
    fn stored(
        &mut self,
        s: &'a DecodedSnapshot,
        v: &'a DecodedValue,
        depth: usize,
    ) -> Result<Value<'a>, Error> {
        match v {
            DecodedValue::Object(id) => self.reference(s, *id, depth),
            DecodedValue::Enum {
                declaration, name, ..
            } => {
                self.bytes(name.len())?;
                Ok(Value::Enum(self.declaration(s, *declaration, true)?, name))
            }
            DecodedValue::OmittedArg => Ok(Value::Omitted),
            DecodedValue::Truncated(reason) => {
                Err(Error::Evidence(Unavailable::Truncated(*reason)))
            }
            DecodedValue::Type(_) => Err(Error::Unsupported),
            DecodedValue::Null => Ok(Value::Scalar(Leaf::Null)),
            DecodedValue::Bool(b) => Ok(Value::Scalar(Leaf::Bool(*b))),
            DecodedValue::Int(n) => Ok(Value::Scalar(Leaf::Int(*n))),
            DecodedValue::Float(f) => Ok(Value::Scalar(Leaf::Float(*f))),
            DecodedValue::String(text) => {
                self.bytes(text.len())?;
                Ok(Value::Scalar(Leaf::Text(text)))
            }
            DecodedValue::Bigint(n) => {
                self.bytes(usize::try_from(n.bits().div_ceil(8)).map_err(|_| Error::Limit)?)?;
                Ok(Value::Scalar(Leaf::Bigint(n)))
            }
            // A blob's root value is stored in it.
            DecodedValue::External(_) | DecodedValue::ExternalNode { .. } => {
                Err(Error::Evidence(super::span::BROKEN_REFERENCE))
            }
        }
    }

    fn declaration(
        &mut self,
        s: &'a DecodedSnapshot,
        id: NodeId,
        expected_enum: bool,
    ) -> Result<&'a DecodedName, Error> {
        match s.object(id) {
            DecodedObject::Declaration { name, is_enum, .. } if *is_enum == expected_enum => {
                // Names, not runtime-local type tags, are the old query identity.
                self.bytes(name.0.to_string().len())?;
                Ok(name)
            }
            DecodedObject::Truncated(reason) => {
                Err(Error::Evidence(Unavailable::Truncated(*reason)))
            }
            DecodedObject::Declaration { .. }
            | DecodedObject::Uint8Array { .. }
            | DecodedObject::List { .. }
            | DecodedObject::Map { .. }
            | DecodedObject::Instance { .. }
            | DecodedObject::Cell(_)
            | DecodedObject::NonSnapshotable
            | DecodedObject::Descriptive { .. } => Err(Error::Unsupported),
        }
    }

    fn fields(
        &mut self,
        s: &'a DecodedSnapshot,
        entries: &'a [(Box<str>, DecodedValue)],
        original_len: u64,
        depth: usize,
    ) -> Result<Fields<'a>, Error> {
        complete(entries.len(), original_len)?;
        let mut fields = BTreeMap::new();
        for (key, v) in entries {
            self.bytes(key.len())?;
            if fields
                .insert(key.as_ref(), self.value(s, v, depth + 1)?)
                .is_some()
            {
                return Err(Error::Unsupported);
            }
        }
        Ok(fields)
    }

    fn object(
        &mut self,
        s: &'a DecodedSnapshot,
        object: &'a DecodedObject,
        depth: usize,
    ) -> Result<Value<'a>, Error> {
        Ok(match object {
            DecodedObject::Uint8Array { data, original_len } => {
                complete(data.len(), *original_len)?;
                self.bytes(data.len())?;
                Value::Uint8Array(data)
            }
            DecodedObject::List {
                items,
                original_len,
                ..
            } => {
                complete(items.len(), *original_len)?;
                Value::List(
                    items
                        .iter()
                        .map(|v| self.value(s, v, depth + 1))
                        .collect::<Result<_, _>>()?,
                )
            }
            DecodedObject::Map {
                entries,
                original_len,
                ..
            } => Value::Map(self.fields(s, entries, *original_len, depth)?),
            DecodedObject::Instance {
                declaration,
                fields,
                original_len,
                ..
            } => {
                let name = self.declaration(s, *declaration, false)?;
                Value::Class(name, self.fields(s, fields, *original_len, depth)?)
            }
            DecodedObject::Cell(v) => self.value(s, v, depth + 1)?,
            DecodedObject::Truncated(reason) => {
                return Err(Error::Evidence(Unavailable::Truncated(*reason)));
            }
            DecodedObject::Declaration { .. }
            | DecodedObject::NonSnapshotable
            | DecodedObject::Descriptive { .. } => return Err(Error::Unsupported),
        })
    }

    fn json(&mut self, json: &'a Json, depth: usize) -> Result<Value<'a>, Error> {
        self.node(depth)?;
        Ok(match json {
            Json::Null => Value::Scalar(Leaf::Null),
            Json::Bool(b) => Value::Scalar(Leaf::Bool(*b)),
            Json::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::Scalar(Leaf::Int(i))
                } else if let Some(i) = n.as_u64() {
                    Value::Unsigned(i)
                } else {
                    Value::Scalar(Leaf::Float(n.as_f64().ok_or(Error::Unsupported)?))
                }
            }
            Json::String(s) => {
                self.bytes(s.len())?;
                Value::Scalar(Leaf::Text(s))
            }
            Json::Array(items) => Value::List(
                items
                    .iter()
                    .map(|v| self.json(v, depth + 1))
                    .collect::<Result<_, _>>()?,
            ),
            Json::Object(entries) => {
                let mut fields = BTreeMap::new();
                for (key, v) in entries {
                    self.bytes(key.len())?;
                    fields.insert(key.as_str(), self.json(v, depth + 1)?);
                }
                Value::Map(fields)
            }
        })
    }
}

fn complete(captured: usize, original: u64) -> Result<(), Error> {
    if captured as u64 != original {
        return Err(Error::Evidence(Unavailable::Truncated(
            btel_snapshot::Limit::Values,
        )));
    }
    Ok(())
}

pub fn captured(
    source: &(impl BlobSource + ?Sized),
    left: Captured<'_>,
    op: CmpOp,
    right: Captured<'_>,
    limits: &Limits,
) -> Result<Option<bool>, Error> {
    if !op.is_equality() {
        let (Some(a), Some(b)) = (leaf_of(left.nav), leaf_of(right.nav)) else {
            return Ok(None);
        };
        if matches!(a, Leaf::Structured) || matches!(b, Leaf::Structured) {
            return Err(Error::Unsupported);
        }
        return Ok(compare(a, op, b));
    }
    let (from_left, from_right) = (Start::of(left.nav), Start::of(right.nav));
    let span = Span::load(
        source,
        from_left.values().into_iter().chain(from_right.values()),
        limits.max_blobs,
    );
    let mut builder = Builder::new(limits, &span);
    let a = builder.captured(&from_left, left.names)?;
    let b = builder.captured(&from_right, right.names)?;
    Ok(a.zip(b).map(|(a, b)| a.equal(&b) == (op == CmpOp::Eq)))
}

pub fn json(
    source: &(impl BlobSource + ?Sized),
    left: Captured<'_>,
    op: CmpOp,
    right: &Json,
    limits: &Limits,
) -> Result<Option<bool>, Error> {
    if !op.is_equality() {
        return Err(Error::Unsupported);
    }
    let from_left = Start::of(left.nav);
    let span = Span::load(source, from_left.values(), limits.max_blobs);
    let mut builder = Builder::new(limits, &span);
    let a = builder.captured(&from_left, left.names)?;
    let b = builder.json(right, 0)?;
    Ok(a.map(|a| a.equal(&b) == (op == CmpOp::Eq)))
}

#[cfg(test)]
mod tests;
