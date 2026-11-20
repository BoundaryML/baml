//! A `reflect.interface.Implementation`'s state.

use indexmap::IndexMap;

use crate::{HeapPtr, RealizedTy};

/// A structural witness, being built, that a runtime-built class implements
/// an interface: the interface, the type it is witnessed at, and the class
/// field each interface field links to so far.
///
/// It holds heap edges, the interface and the heads of its type, which the
/// collector traces through the instance that holds it.
#[derive(Clone, Debug)]
pub struct InterfaceWitness {
    /// The `Object::Interface` witnessed.
    pub interface_ptr: HeapPtr,
    /// The interface at the type arguments it is witnessed at.
    pub interface_ty: RealizedTy,
    /// Each interface field linked so far, to the class field satisfying it.
    pub field_links: IndexMap<baml_type::Name, baml_type::Name>,
}
