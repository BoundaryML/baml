//! Retained interface capabilities. These are session-owned values, not data.

use crate::Handle;
use baml_type::{RealizedTy, TaggedTypeName, typetag::TypeTag};
use std::{collections::BTreeMap, sync::Arc};

/// Receiver storage independent of whether the implementor is a class.
#[derive(Clone, Debug, PartialEq)]
pub enum InterfaceReceiver {
    Null,
    Int(i64),
    Bool(bool),
    Heap(Handle),
}

/// An engine-checked view. All declaration references are owned GC handles;
/// no movable VM pointer is allowed to survive a boundary in this value.
///
/// Construction is engine-internal in intent. Every adoption revalidates heap
/// provenance and declaration identity; host-side annotations are not proof.
#[derive(Clone, Debug, PartialEq)]
pub struct InterfaceValue {
    pub receiver: InterfaceReceiver,
    pub interface: RealizedTy<TaggedTypeName>,
    pub declarations: Arc<BTreeMap<TypeTag, Handle>>,
    /// The dynamic implementation world, if any. Static program rules remain
    /// owned by the runtime. A receiver keeps its own authority on pass-back.
    pub world: Option<Handle>,
}
