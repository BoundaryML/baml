//! Operator dispatch through interfaces (README decision 4): `a + b` IS
//! `Implements(lhs, baml.ops.Add<rhs>)` with the impl's `Output` as the
//! result. Since I1 this is a thin facade over the real impl registry
//! (`crate::impls`) - blanket impls, in-class implements blocks, and
//! user-defined operator impls all resolve through the same path as
//! every other interface fact. Rewriting primitive cases to single
//! instructions is MIR's job at lowering, invisible to inference.

use baml_type::{
    Name, TypeName,
    interned::{InterfaceRef, Ty},
};

use crate::impls::{resolve_impl, resolved_pin};

/// The `Output` of the unique impl matching `baml.ops.<interface><rhs>`
/// for `lhs`, or `None` when the operands do not support the operator.
/// Operands must be resolved and literal-widened by the caller.
pub fn operator_output(
    db: &dyn baml_compiler2_ppir::Db,
    interface: &str,
    lhs: &Ty,
    rhs: Option<&Ty>,
) -> Option<Ty> {
    let target = InterfaceRef::new(
        TypeName::new(
            Name::new("baml"),
            vec![Name::new("ops")],
            Name::new(interface),
        ),
        rhs.cloned()
            .into_iter()
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        Vec::new(),
    );
    let resolved = resolve_impl(db, lhs, &target)?;
    // Binding-else-default through the shared `leaf_def` read, so an
    // operator interface with a defaulted `Output` resolves like any
    // other associated member.
    resolved_pin(db, &resolved, lhs, &Name::new("Output"))
}
