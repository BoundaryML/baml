//! Instances of `@@rust_type` classes.
//!
//! A class tagged `@@rust_type` keeps its state in Rust rather than in BAML
//! fields. Its instance is one heap object: the class it belongs to, the type
//! arguments it was built at, and the state itself, which no BAML code can
//! name, read, or reassign. That is what keeps the state tied to exactly one
//! class at exactly its type arguments: nothing can move an `OrderedSet<int>`'s
//! members under an `OrderedSet<string>`, because there is no field to move.
//!
//! The kind of state is one [`RustPayload`] variant per tagged kind, and every
//! behavior a heap value has — tracing, copying, comparing, hashing,
//! rendering, crossing to the host — matches on it exhaustively, so a new kind
//! is a decision at each of them rather than a fallback.

use std::sync::Arc;

use crate::{BexRustData, HeapPtr};

/// The instance of a `@@rust_type` class.
///
/// The class and type arguments are what [`Instance`](super::Instance) holds;
/// the payload stands in for the fields a plain class would have.
#[derive(Clone, Debug)]
pub struct RustInstance {
    /// The `Object::Class` this is an instance of.
    pub class: HeapPtr,
    /// The class's type arguments, realized at construction, in the class's
    /// parameter order. Empty for a non-generic class.
    pub class_type_args: Box<[crate::RealizedTy]>,
    pub payload: RustPayload,
}

/// The state behind a `@@rust_type` instance, one variant per kind.
///
/// A variant decides its own ownership. State the VM owns is a plain value,
/// and a deep copy copies it; state shared with something outside the VM — a
/// detached task, a host — holds an `Arc` inside the variant, and a deep copy
/// shares it.
#[derive(Clone, Debug)]
pub enum RustPayload {
    /// A value the VM cannot look inside, reached only by downcasting its
    /// `Arc`. Transitional: it stands in for every kind that has no variant
    /// of its own yet, and goes away once each has one.
    Foreign(Arc<dyn BexRustData>),
}

impl RustInstance {
    pub fn new(
        class: HeapPtr,
        class_type_args: Box<[crate::RealizedTy]>,
        payload: RustPayload,
    ) -> Self {
        Self {
            class,
            class_type_args,
            payload,
        }
    }
}

impl std::fmt::Display for RustInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<rust instance of {:p}>", self.class.as_ptr())
    }
}
