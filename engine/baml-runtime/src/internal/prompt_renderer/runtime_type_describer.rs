use internal_baml_core::ir::repr::IntermediateRepr;

use crate::RuntimeContext;

trait RuntimeTypeLibrary {}

/// This class contains all the type
/// definitions that are used in the runtime.
/// This can be types defined at runtime or in the IR.
struct RuntimeTypeDefintions<'a> {
    // The type definitions that are used in the runtime.
    // This can be types defined at runtime or in the IR.
    ir: &'a IntermediateRepr,
    ctx: &'a RuntimeContext,
}

impl<'a> RuntimeTypeDefintions<'a> {
    /// Create a new instance of the `RuntimeTypeDefintions` class.
    fn new(ir: &'a IntermediateRepr, ctx: &'a RuntimeContext) -> Self {
        RuntimeTypeDefintions { ir, ctx }
    }
}

impl RuntimeTypeLibrary for RuntimeTypeDefintions<'_> {}
