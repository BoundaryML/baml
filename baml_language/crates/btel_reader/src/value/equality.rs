//! Bounded semantic equality, independent of CAS IDs, object IDs and rendering.
//! Build borrowed trees so sharing is immaterial; cycles and opaque values are
//! explicitly unavailable. Validate both operands before deciding inequality.
use std::collections::{BTreeMap, HashSet};

use btel_snapshot::{DecodedName, DecodedObject, DecodedRoot, DecodedSnapshot, DecodedValue};
use serde_json::Value as Json;

use super::{CmpOp, Leaf, Nav, Unavailable, compare, leaf_of};
use crate::evidence::ArgumentNames;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_depth: usize,
    pub max_nodes: usize,
    /// Text, keys and bytes examined across both operands, including sharing.
    pub max_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_depth: 64,
            max_nodes: 100_000,
            max_bytes: 16 << 20,
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
    pub snapshot: &'a DecodedSnapshot,
    pub nav: Nav<'a>,
    pub names: Option<&'a ArgumentNames>,
}

type Fields<'a> = BTreeMap<&'a str, Value<'a>>;

enum Value<'a> {
    Scalar(Leaf<'a>),
    Unsigned(u64),
    Omitted,
    Bytes(&'a [u8]),
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
            (Self::Bytes(a), Self::Bytes(b)) => a == b,
            (Self::Enum(a, x), Self::Enum(b, y)) => a == b && x == y,
            (Self::List(a), Self::List(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.equal(y))
            }
            (Self::Map(a), Self::Map(b)) => fields_equal(a, b),
            (Self::Class(a, x), Self::Class(b, y)) => a == b && fields_equal(x, y),
            _ => false,
        }
    }
}

fn fields_equal(a: &Fields<'_>, b: &Fields<'_>) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|((k, x), (l, y))| k == l && x.equal(y))
}

struct Builder<'a> {
    limits: &'a Limits,
    nodes: usize,
    bytes: usize,
    active: HashSet<u32>,
}

impl<'a> Builder<'a> {
    fn new(limits: &'a Limits) -> Self {
        Self {
            limits,
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

    fn captured<'b>(&mut self, value: Captured<'b>) -> Result<Option<Value<'b>>, Error> {
        match value.nav {
            Nav::Missing => Ok(None),
            Nav::Unavailable(reason) => Err(Error::Evidence(reason)),
            Nav::Value(v) => self.value(value.snapshot, v, 0).map(Some),
            Nav::Arguments => {
                let DecodedRoot::FunctionArgs {
                    parameter_count,
                    slots,
                } = &value.snapshot.root
                else {
                    return Err(Error::Evidence(Unavailable::WrongRoot));
                };
                complete(slots.len(), *parameter_count)?;
                let names = value
                    .names
                    .ok_or(Error::Evidence(Unavailable::ArgumentNamesUnknown))?;
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
                    if fields
                        .insert(name, self.value(value.snapshot, v, 1)?)
                        .is_some()
                    {
                        return Err(Error::Evidence(Unavailable::ArgumentLayoutMismatch));
                    }
                }
                Ok(Some(Value::Map(fields)))
            }
        }
    }

    fn value<'b>(
        &mut self,
        s: &'b DecodedSnapshot,
        v: &'b DecodedValue,
        depth: usize,
    ) -> Result<Value<'b>, Error> {
        self.node(depth)?;
        match v {
            DecodedValue::Object(id) => {
                if !self.active.insert(*id) {
                    return Err(Error::Cycle);
                }
                let result = self.object(s, s.object(*id), depth);
                self.active.remove(id);
                result
            }
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
            _ => {
                match v {
                    DecodedValue::String(text) => self.bytes(text.len())?,
                    DecodedValue::Bigint(n) => self
                        .bytes(usize::try_from(n.bits().div_ceil(8)).map_err(|_| Error::Limit)?)?,
                    _ => {}
                }
                Ok(Value::Scalar(leaf_of(Nav::Value(v)).expect("scalar value")))
            }
        }
    }

    fn declaration<'b>(
        &mut self,
        s: &'b DecodedSnapshot,
        id: u32,
        expected_enum: bool,
    ) -> Result<&'b DecodedName, Error> {
        match s.object(id) {
            DecodedObject::Declaration { name, is_enum, .. } if *is_enum == expected_enum => {
                // Names, not runtime-local type tags, are the old query identity.
                self.bytes(name.0.to_string().len())?;
                Ok(name)
            }
            DecodedObject::Truncated(reason) => {
                Err(Error::Evidence(Unavailable::Truncated(*reason)))
            }
            _ => Err(Error::Unsupported),
        }
    }

    fn fields<'b>(
        &mut self,
        s: &'b DecodedSnapshot,
        entries: &'b [(Box<str>, DecodedValue)],
        original_len: u64,
        depth: usize,
    ) -> Result<Fields<'b>, Error> {
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

    fn object<'b>(
        &mut self,
        s: &'b DecodedSnapshot,
        object: &'b DecodedObject,
        depth: usize,
    ) -> Result<Value<'b>, Error> {
        Ok(match object {
            DecodedObject::Bytes { data, original_len } => {
                complete(data.len(), *original_len)?;
                self.bytes(data.len())?;
                Value::Bytes(data)
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
            _ => return Err(Error::Unsupported),
        })
    }

    fn json<'b>(&mut self, json: &'b Json, depth: usize) -> Result<Value<'b>, Error> {
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
    let mut builder = Builder::new(limits);
    let a = builder.captured(left)?;
    let b = builder.captured(right)?;
    Ok(a.zip(b).map(|(a, b)| a.equal(&b) == (op == CmpOp::Eq)))
}

pub fn json(
    left: Captured<'_>,
    op: CmpOp,
    right: &Json,
    limits: &Limits,
) -> Result<Option<bool>, Error> {
    if !op.is_equality() {
        return Err(Error::Unsupported);
    }
    let mut builder = Builder::new(limits);
    let a = builder.captured(left)?;
    let b = builder.json(right, 0)?;
    Ok(a.map(|a| a.equal(&b) == (op == CmpOp::Eq)))
}

#[cfg(test)]
mod tests;
