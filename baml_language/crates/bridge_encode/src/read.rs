//! Reading a host value into a snapshot, once.
//!
//! [`HostValue::shape`] is independent of the slot a value is planned
//! against, so the whole value is read before any planning: every child is
//! asked once, in document order, and the planner then works on the
//! [`Tree`] alone and can call no host code. Reading is where a host's own
//! exception surfaces, and where a value too large or too deep for one
//! request is refused before anything is planned.

use bridge_ctypes::WIRE_NESTING_LIMIT;

use crate::{
    error::{EncodeError, EncodeErrorKind, Error, FaultKind, PathSegment},
    host::{HostValue, Shape},
};

/// One host value, read once, with its children.
pub(crate) struct Tree<V> {
    /// The live host object: the owner of any handle, string or payload the
    /// wire will carry, kept alive until commit.
    pub value: V,
    pub shape: Shape<Tree<V>>,
}

/// A value nested deeper than this cannot cross: a list level alone costs two
/// of the wire's [`WIRE_NESTING_LIMIT`] messages. The exact measure is taken
/// of the committed wire; this bound only keeps the read finite for a cyclic
/// value.
pub(crate) const MAX_VALUE_DEPTH: usize = WIRE_NESTING_LIMIT / 2;

/// How many nodes one request may carry. A shared sub-value is copied per
/// reference on the wire (the wire carries trees), so a small graph can
/// expand without bound; this keeps it finite.
pub(crate) const NODE_BUDGET: usize = 1 << 20;

/// Read `value` and everything under it. `root` names where it sits, for an
/// error's path.
pub(crate) fn read<V: HostValue>(
    root: PathSegment,
    value: V,
    budget: &mut usize,
) -> Result<Tree<V>, Error<V::Error>> {
    let mut reader = Reader {
        path: vec![root],
        budget,
    };
    reader.node(value)
}

struct Reader<'b> {
    path: Vec<PathSegment>,
    budget: &'b mut usize,
}

impl Reader<'_> {
    fn fault<E>(&self, kind: FaultKind) -> Error<E> {
        Error::Value(EncodeError {
            path: self.path.clone(),
            kind: Box::new(EncodeErrorKind::Fault(kind)),
        })
    }

    fn node<V: HostValue>(&mut self, value: V) -> Result<Tree<V>, Error<V::Error>> {
        if self.path.len() > MAX_VALUE_DEPTH {
            return Err(self.fault(FaultKind::TooDeep {
                limit: WIRE_NESTING_LIMIT,
            }));
        }
        match self.budget.checked_sub(1) {
            Some(left) => *self.budget = left,
            None => {
                return Err(self.fault(FaultKind::TooLarge {
                    budget: NODE_BUDGET,
                }));
            }
        }
        let shape = match value.shape().map_err(Error::Host)? {
            Shape::List(items) => {
                let mut read = Vec::with_capacity(items.len());
                for (index, item) in items.into_iter().enumerate() {
                    read.push(self.within(PathSegment::Index(index), item)?);
                }
                Shape::List(read)
            }
            Shape::Map(entries) => {
                let mut read = Vec::with_capacity(entries.len());
                for (index, (key, item)) in entries.into_iter().enumerate() {
                    // The key's text is not known until it is read; its
                    // position names it in an error about the key itself.
                    let key = self.within(PathSegment::Key(index.to_string()), key)?;
                    let item = self.within(PathSegment::Key(index.to_string()), item)?;
                    read.push((key, item));
                }
                Shape::Map(read)
            }
            Shape::Instance {
                class,
                type_args,
                fields,
            } => {
                let mut read = Vec::with_capacity(fields.len());
                for (name, field) in fields {
                    let field = self.within(PathSegment::Field(name.clone()), field)?;
                    read.push((name, field));
                }
                Shape::Instance {
                    class,
                    type_args,
                    fields: read,
                }
            }
            Shape::Null => Shape::Null,
            Shape::Scalar(scalar) => Shape::Scalar(scalar),
            Shape::EnumMember {
                enumeration,
                variant,
            } => Shape::EnumMember {
                enumeration,
                variant,
            },
            Shape::Callable => Shape::Callable,
            Shape::Leaf(kind) => Shape::Leaf(kind),
            Shape::Other => Shape::Other,
        };
        Ok(Tree { value, shape })
    }

    fn within<V: HostValue>(
        &mut self,
        segment: PathSegment,
        value: V,
    ) -> Result<Tree<V>, Error<V::Error>> {
        self.path.push(segment);
        let tree = self.node(value);
        self.path.pop();
        tree
    }
}
